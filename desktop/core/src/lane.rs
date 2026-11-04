//! One step in Program Files at a time (reel-elevate.exe as administrator): every request goes
//! through one worker in order, and saves that wait together are taken as one.
use crate::elevated::Step;
use crate::settings::Settings;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

pub const PASSED: &str = "a newer change came in after this one, so this one was passed over";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Doing {
    pub step: &'static str,
    /// "asking" while Windows' prompt is open, "working" while the helper sets the copy up.
    pub phase: &'static str,
    pub line: String,
}

impl Doing {
    pub fn asking(step: Step) -> Doing {
        Doing {
            step: step.word(),
            phase: "asking",
            line: format!("Windows asks for the rights to {}", what(step)),
        }
    }

    pub fn working(step: Step) -> Doing {
        Doing {
            step: step.word(),
            phase: "working",
            line: match step {
                Step::Install => "setting up the copy of OBS in Program Files",
                Step::Write => "writing the setup into the copy of OBS in Program Files",
                Step::Remove => "removing the copy of OBS from Program Files",
            }
            .into(),
        }
    }
}

fn what(step: Step) -> &'static str {
    match step {
        Step::Install => "set up the copy of OBS in Program Files",
        Step::Write => "write the setup into the copy of OBS in Program Files",
        Step::Remove => "remove the copy of OBS from Program Files",
    }
}

/// Whether the settings run OBS for games that run as administrator.
pub fn on(s: &Settings) -> bool {
    s.obs.bundled && s.obs.setup.elevated
}

/// `structural` is a change of the setup while the switch is on.
pub fn step_for(was: bool, on: bool, structural: bool, copy_there: bool) -> Option<Step> {
    match (was, on) {
        (false, true) => Some(Step::Install),
        (true, false) => Some(Step::Remove),
        (true, true) if structural => Some(if copy_there {
            Step::Write
        } else {
            Step::Install
        }),
        _ => None,
    }
}

pub fn plan(old: &Settings, new: &Settings, copy_there: bool) -> Option<Step> {
    step_for(on(old), on(new), structural(old, new), copy_there)
}

/// A change the copy's own settings hold: the setup but its levels and filters, which go over
/// the WebSocket; the clips folder; the Save hotkey.
fn structural(old: &Settings, new: &Settings) -> bool {
    let mut a = old.obs.setup.clone();
    let mut b = new.obs.setup.clone();
    for s in [&mut a, &mut b] {
        s.volumes.clear();
        s.noise = Default::default();
        s.gate = false;
        s.own_filters.clear();
        s.own_filters_from.clear();
        s.done = true;
    }
    a != b || new.clips_dir != old.clips_dir || new.hotkeys.save != old.hotkeys.save
}

/// On a failure the switch stays as it was, but off when a removal failed after the copy had
/// gone, and a change of the setup is not kept.
pub fn after(
    step: Step,
    old: &Settings,
    mut new: Settings,
    done: Result<(), String>,
    copy_there: bool,
) -> (Settings, Option<String>) {
    let Err(e) = done else {
        return (new, None);
    };
    let said = match step {
        Step::Install if !on(old) => {
            new.obs.setup.elevated = false;
            format!("Capture games that run as administrator stayed off: {e}")
        }
        Step::Remove if copy_there => {
            new.obs.setup.elevated = true;
            new.obs.bundled = old.obs.bundled;
            format!("the OBS for games that run as administrator was not removed, so it stays on: {e}")
        }
        Step::Remove => format!(
            "the copy of OBS is gone from Program Files, so the switch is off, but not all of the removal went through: {e}"
        ),
        _ => {
            let elevated = new.obs.setup.elevated;
            new.obs.setup = old.obs.setup.clone();
            new.obs.setup.elevated = elevated;
            new.clips_dir = old.clips_dir.clone();
            new.hotkeys.save = old.hotkeys.save.clone();
            format!("the change was not written into the OBS for games that run as administrator, so the setup stays as it was: {e}")
        }
    };
    (new, Some(said))
}

/// A copy or its task left with the switch off: its task would start OBS with the rights at
/// sign-in beside reel's own.
pub fn left(on: bool, copy_there: bool, task: bool) -> bool {
    !on && (copy_there || task)
}

/// The settings' leaves a request changed, so a request that waited applies over what the
/// steps before it left.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes(Vec<(Vec<String>, Option<Value>)>);

impl Changes {
    pub fn between(base: &Settings, new: &Settings) -> Changes {
        let mut out = Vec::new();
        diff(&mut Vec::new(), &json(base), &json(new), &mut out);
        Changes(out)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether it turns OBS for games that run as administrator, or reel's OBS, on or off.
    pub fn switches(&self) -> bool {
        self.0
            .iter()
            .any(|(p, _)| is(p, &["obs", "bundled"]) || is(p, &["obs", "setup", "elevated"]))
    }

    pub fn onto(&self, s: &Settings) -> Result<Settings, String> {
        let mut v = json(s);
        for (path, value) in &self.0 {
            put(&mut v, path, value.clone());
        }
        serde_json::from_value(v).map_err(|e| format!("the settings as changed: {e}"))
    }
}

fn is(path: &[String], want: &[&str]) -> bool {
    path.len() == want.len() && path.iter().zip(want).all(|(a, b)| a == b)
}

fn json(s: &Settings) -> Value {
    serde_json::to_value(s).unwrap_or(Value::Null)
}

fn diff(path: &mut Vec<String>, a: &Value, b: &Value, out: &mut Vec<(Vec<String>, Option<Value>)>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, bv) in y {
                path.push(k.clone());
                match x.get(k) {
                    Some(av) => diff(path, av, bv, out),
                    None => out.push((path.clone(), Some(bv.clone()))),
                }
                path.pop();
            }
            for k in x.keys().filter(|k| !y.contains_key(*k)) {
                path.push(k.clone());
                out.push((path.clone(), None));
                path.pop();
            }
        }
        _ if a != b => out.push((path.clone(), Some(b.clone()))),
        _ => {}
    }
}

fn put(v: &mut Value, path: &[String], value: Option<Value>) {
    let Some((last, parents)) = path.split_last() else {
        if let Some(x) = value {
            *v = x;
        }
        return;
    };
    let mut at = v;
    for k in parents {
        if !at.is_object() {
            *at = Value::Object(Map::new());
        }
        let Value::Object(m) = at else { return };
        at = m.entry(k.clone()).or_insert(Value::Object(Map::new()));
    }
    if !at.is_object() {
        *at = Value::Object(Map::new());
    }
    let Value::Object(m) = at else { return };
    match value {
        Some(x) => {
            m.insert(last.clone(), x);
        }
        None => {
            m.remove(last);
        }
    }
}

/// What a step is for: a save of the settings, Update the copy, or Remove it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Save,
    Again,
    Remove,
}

pub enum Ask {
    Save(Changes),
    Again,
    Remove,
    /// The copy read and set against the saved setting once nothing waits.
    Check,
    /// Anything else that writes the settings, such as what the app read from OBS.
    Run(Box<dyn FnOnce() + Send>),
}

pub type Answer = Result<Settings, String>;

/// What the worker drives: the app, or a stand-in in the tests.
pub trait Program: Send + Sync + 'static {
    /// OBS held stopped while it lives.
    type Hold: Send;
    fn saved(&self) -> Settings;
    /// The app came with OBS: the switch has a copy in Program Files behind it.
    fn with_obs(&self) -> bool;
    fn copy_there(&self) -> bool;
    /// OBS stopped, once it is out; None without reel's OBS.
    fn hold(&self) -> Option<Self::Hold>;
    /// The step, Windows' prompt first; `answered` once the prompt is.
    fn step(
        &self,
        job: Job,
        step: Step,
        old: &Settings,
        new: &Settings,
        answered: &dyn Fn(),
    ) -> Result<(), String>;
    /// The settings kept, and what follows from them in the rest of the app.
    fn save(&self, old: &Settings, new: Settings) -> Answer;
    /// The line under the switch after a step: why it did not go through, or nothing.
    fn said(&self, problem: Option<String>);
    /// Nothing waits: the copy read again and set against the saved setting.
    fn settle(&self);
    /// What the pages show changed: the step, its phase, the lane idle.
    fn changed(&self);
}

struct Waiting {
    ask: Ask,
    reply: Option<mpsc::Sender<Answer>>,
}

type Member = (Changes, Option<mpsc::Sender<Answer>>);

#[derive(Default)]
struct State {
    queue: VecDeque<Waiting>,
    doing: Option<Doing>,
    busy: bool,
}

/// The requests that may run a step, one worker for them all.
pub struct Lane {
    state: Mutex<State>,
    more: Condvar,
    /// How long OBS stays stopped after the last step for another request.
    linger: Duration,
}

impl Lane {
    pub fn new(linger: Duration) -> Lane {
        Lane {
            state: Mutex::default(),
            more: Condvar::new(),
            linger,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn start<P: Program>(self: &Arc<Self>, p: P) {
        let lane = self.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("reel-steps".into())
            .spawn(move || lane.work(&p))
        {
            tracing::error!(error = %e, "the worker for the steps in Program Files did not start");
        }
    }

    /// The request, in its turn; its answer once the turn is over.
    pub fn ask(&self, ask: Ask) -> Answer {
        let (tx, rx) = mpsc::channel();
        self.push(ask, Some(tx));
        rx.recv()
            .unwrap_or_else(|_| Err("the step went wrong; the app's log says why".into()))
    }

    /// The request, in its turn, answered to nobody.
    pub fn post(&self, ask: Ask) {
        self.push(ask, None);
    }

    fn push(&self, ask: Ask, reply: Option<mpsc::Sender<Answer>>) {
        self.lock().queue.push_back(Waiting { ask, reply });
        self.more.notify_all();
    }

    pub fn doing(&self) -> Option<Doing> {
        self.lock().doing.clone()
    }

    /// Nothing runs, nothing waits, OBS is not held for the steps and the copy is checked.
    pub fn idle(&self) -> bool {
        let s = self.lock();
        !s.busy && s.queue.is_empty()
    }

    pub fn queued(&self) -> usize {
        self.lock().queue.len()
    }

    fn set(&self, doing: Option<Doing>) {
        self.lock().doing = doing;
    }

    /// The saves at the front of the queue, taken into the one at work.
    fn absorb(&self, into: &mut Vec<Member>) -> usize {
        let mut s = self.lock();
        let mut n = 0;
        while matches!(
            s.queue.front(),
            Some(Waiting {
                ask: Ask::Save(_),
                ..
            })
        ) {
            if let Some(Waiting {
                ask: Ask::Save(c),
                reply,
            }) = s.queue.pop_front()
            {
                into.push((c, reply));
                n += 1;
            }
        }
        n
    }

    /// A newer request waits that turns the switch, or runs a step of its own.
    fn overtaken(&self) -> bool {
        self.lock().queue.iter().any(|w| match &w.ask {
            Ask::Save(c) => c.switches(),
            Ask::Again | Ask::Remove => true,
            Ask::Check | Ask::Run(_) => false,
        })
    }

    fn work<P: Program>(&self, p: &P) {
        let mut held: Option<P::Hold> = None;
        let mut settle = false;
        loop {
            let next = {
                let mut s = self.lock();
                let n = s.queue.pop_front();
                if n.is_some() {
                    s.busy = true;
                }
                n
            };
            let Some(next) = next else {
                if held.is_some() {
                    let s = self.lock();
                    let (s, _) = self
                        .more
                        .wait_timeout_while(s, self.linger, |s| s.queue.is_empty())
                        .unwrap_or_else(|e| e.into_inner());
                    if !s.queue.is_empty() {
                        continue;
                    }
                }
                // OBS starts once and the copy is settled before the pages hear the lane is idle.
                drop(held.take());
                if std::mem::take(&mut settle) {
                    p.settle();
                }
                {
                    let mut s = self.lock();
                    if !s.queue.is_empty() {
                        continue;
                    }
                    s.busy = false;
                }
                p.changed();
                let mut s = self.lock();
                while s.queue.is_empty() {
                    s = self.more.wait(s).unwrap_or_else(|e| e.into_inner());
                }
                continue;
            };
            settle |= !matches!(next.ask, Ask::Run(_));
            let unit = std::panic::catch_unwind(AssertUnwindSafe(|| match next.ask {
                Ask::Save(c) => self.save(p, vec![(c, next.reply)], &mut held),
                Ask::Again => self.copy(p, Job::Again, next.reply, &mut held),
                Ask::Remove => self.copy(p, Job::Remove, next.reply, &mut held),
                Ask::Check => answer(next.reply, Ok(p.saved())),
                Ask::Run(f) => {
                    f();
                    answer(next.reply, Ok(p.saved()))
                }
            }));
            if unit.is_err() {
                self.set(None);
                tracing::error!("a request for a step in Program Files went wrong");
            }
        }
    }

    fn step<P: Program>(
        &self,
        p: &P,
        job: Job,
        step: Step,
        old: &Settings,
        new: &Settings,
    ) -> Result<(), String> {
        self.set(Some(Doing::asking(step)));
        p.changed();
        let done = p.step(job, step, old, new, &|| {
            self.set(Some(Doing::working(step)));
            p.changed();
        });
        self.set(None);
        done
    }

    fn save<P: Program>(&self, p: &P, mut members: Vec<Member>, held: &mut Option<P::Hold>) {
        self.absorb(&mut members);
        let old = p.saved();
        let mut target = merged(&old, &mut members);
        let mut step = plan_with(p, &old, &target);
        if step.is_some() {
            if held.is_none() {
                *held = p.hold();
            }
            // The saves that came while OBS stopped: Windows has not asked yet.
            if self.absorb(&mut members) > 0 {
                target = merged(&old, &mut members);
                step = plan_with(p, &old, &target);
            }
        }
        let fin = match step {
            None => target,
            Some(st) => {
                let done = self.step(p, Job::Save, st, &old, &target);
                let (fin, said) = after(st, &old, target, done, p.copy_there());
                p.said(said);
                fin
            }
        };
        let saved = p.save(&old, fin);
        for (_, r) in members {
            answer(r, saved.clone());
        }
    }

    fn copy<P: Program>(
        &self,
        p: &P,
        job: Job,
        reply: Option<mpsc::Sender<Answer>>,
        held: &mut Option<P::Hold>,
    ) {
        let done = (|| {
            if self.overtaken() {
                return Err(PASSED.to_string());
            }
            let s = p.saved();
            if !p.with_obs() {
                return Err("this app came without OBS".into());
            }
            match job {
                Job::Again if !on(&s) => {
                    return Err("Capture games that run as administrator is off".into())
                }
                Job::Remove if on(&s) => return Err(
                    "Capture games that run as administrator is on; turn it off to remove the copy"
                        .into(),
                ),
                _ => {}
            }
            if held.is_none() {
                *held = p.hold();
            }
            if self.overtaken() {
                return Err(PASSED.to_string());
            }
            let step = if job == Job::Remove {
                Step::Remove
            } else {
                Step::Install
            };
            let done = self.step(p, job, step, &s, &s);
            p.said(done.as_ref().err().map(|e| match job {
                Job::Remove => format!("the copy left in Program Files was not removed: {e}"),
                _ => e.clone(),
            }));
            p.changed();
            done.map(|_| s)
        })();
        answer(reply, done);
    }
}

fn answer(reply: Option<mpsc::Sender<Answer>>, a: Answer) {
    if let Some(r) = reply {
        let _ = r.send(a);
    }
}

fn plan_with<P: Program>(p: &P, old: &Settings, new: &Settings) -> Option<Step> {
    if !p.with_obs() {
        return None;
    }
    plan(old, new, p.copy_there())
}

/// One that does not apply is answered at once and left out.
fn merged(old: &Settings, members: &mut Vec<Member>) -> Settings {
    let mut s = old.clone();
    members.retain(|(c, r)| match c.onto(&s) {
        Ok(n) => {
            s = n;
            true
        }
        Err(e) => {
            if let Some(r) = r {
                let _ = r.send(Err(e));
            }
            false
        }
    });
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use std::time::Instant;

    fn with(elevated: bool) -> Settings {
        let mut s = Settings::default();
        s.obs.bundled = true;
        s.obs.setup.elevated = elevated;
        s
    }

    fn switched(base: bool, want: bool) -> Changes {
        Changes::between(&with(base), &with(want))
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Reply {
        Yes,
        No,
        Fails,
        FailsHalfway,
    }

    #[derive(Default)]
    struct W {
        saved: Settings,
        copy: bool,
        task: bool,
        obs: Option<(bool, Instant)>,
        held: u32,
        holds: (u32, u32),
        inside: u32,
        most_inside: u32,
        prompts: u32,
        slow_closes: u32,
        starts: u32,
        starts_while_waiting: u32,
        two_obs: u32,
        left: bool,
        checks: u32,
        checks_mid: u32,
        replies: VecDeque<Reply>,
    }

    #[derive(Clone, Copy, Debug)]
    struct St {
        step: bool,
        on: bool,
    }

    enum Ev {
        Status(St),
        Settings(bool),
        Answered(Box<Answer>),
        Click,
        Seen(mpsc::Sender<(bool, u32)>),
    }

    struct World {
        w: Mutex<W>,
        u: Duration,
        waiting: OnceLock<Box<dyn Fn() -> usize + Send + Sync>>,
        lane: OnceLock<Arc<Lane>>,
        page: Mutex<Option<mpsc::Sender<Ev>>>,
    }

    impl World {
        fn new(u: Duration, on: bool, obs_since: Instant) -> Arc<World> {
            Arc::new(World {
                w: Mutex::new(W {
                    saved: with(on),
                    copy: on,
                    task: on,
                    obs: Some((on, obs_since)),
                    ..W::default()
                }),
                u,
                waiting: OnceLock::new(),
                lane: OnceLock::new(),
                page: Mutex::new(None),
            })
        }

        fn lock(&self) -> MutexGuard<'_, W> {
            self.w.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn waiting(&self) -> usize {
            self.waiting.get().map(|f| f()).unwrap_or(0)
        }

        fn tell(&self, ev: Ev) {
            if let Some(p) = self.page.lock().unwrap().as_ref() {
                let _ = p.send(ev);
            }
        }
    }

    struct Held(Arc<World>);

    impl Drop for Held {
        fn drop(&mut self) {
            let waiting = self.0.waiting();
            let mut w = self.0.lock();
            w.held -= 1;
            w.holds.1 += 1;
            if w.held == 0 {
                if w.obs.is_some() {
                    w.two_obs += 1;
                }
                if waiting > 0 {
                    w.starts_while_waiting += 1;
                }
                w.starts += 1;
                let mode = on(&w.saved);
                w.obs = Some((mode, Instant::now()));
            }
        }
    }

    impl Program for Arc<World> {
        type Hold = Held;

        fn saved(&self) -> Settings {
            self.lock().saved.clone()
        }

        fn with_obs(&self) -> bool {
            true
        }

        fn copy_there(&self) -> bool {
            self.lock().copy
        }

        fn hold(&self) -> Option<Held> {
            let young = {
                let w = self.lock();
                w.obs.is_some_and(|(_, t)| t.elapsed() < self.u * 4)
            };
            std::thread::sleep(if young { self.u * 10 } else { self.u / 2 });
            let mut w = self.lock();
            if young {
                w.slow_closes += 1;
            }
            w.obs = None;
            w.held += 1;
            w.holds.0 += 1;
            Some(Held(self.clone()))
        }

        fn step(
            &self,
            _job: Job,
            step: Step,
            _old: &Settings,
            _new: &Settings,
            answered: &dyn Fn(),
        ) -> Result<(), String> {
            let reply = {
                let mut w = self.lock();
                w.inside += 1;
                w.most_inside = w.most_inside.max(w.inside);
                w.prompts += 1;
                w.replies.pop_front().unwrap_or(Reply::Yes)
            };
            std::thread::sleep(self.u / 2);
            if reply != Reply::No {
                answered();
                std::thread::sleep(self.u * 3);
            }
            let mut w = self.lock();
            if matches!(reply, Reply::Yes | Reply::FailsHalfway) {
                let made = step != Step::Remove;
                w.copy = made;
                w.task = made;
            }
            w.inside -= 1;
            match reply {
                Reply::Yes => Ok(()),
                Reply::No => Err("Windows' prompt was declined".into()),
                _ => Err("the helper failed".into()),
            }
        }

        fn save(&self, _old: &Settings, new: Settings) -> Answer {
            self.lock().saved = new.clone();
            self.changed();
            self.tell(Ev::Settings(new.obs.setup.elevated));
            Ok(new)
        }

        fn said(&self, _problem: Option<String>) {}

        fn settle(&self) {
            let waiting = self.waiting();
            let mut w = self.lock();
            w.checks += 1;
            if w.inside > 0 || waiting > 0 {
                w.checks_mid += 1;
            }
            w.left = left(on(&w.saved), w.copy, w.task);
        }

        fn changed(&self) {
            let step = self.lane.get().is_some_and(|l| l.doing().is_some());
            let on = self.lock().saved.obs.setup.elevated;
            self.tell(Ev::Status(St { step, on }));
        }
    }

    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    fn page(lane: Arc<Lane>, world: &Arc<World>, mut seed: u64) -> mpsc::Sender<Ev> {
        let (tx, rx) = mpsc::channel::<Ev>();
        *world.page.lock().unwrap() = Some(tx.clone());
        let back = tx.clone();
        let u = world.u;
        let start = world.lock().saved.obs.setup.elevated;
        std::thread::spawn(move || {
            let (mut shown, mut base, mut asks, mut status) = (start, start, 0u32, None::<St>);
            for ev in rx {
                match ev {
                    Ev::Status(st) => {
                        status = Some(st);
                        if asks == 0 {
                            (shown, base) = (st.on, st.on);
                        }
                    }
                    Ev::Settings(on) => {
                        if asks == 0 {
                            (shown, base) = (on, on);
                        }
                    }
                    Ev::Click => {
                        if asks > 0 || status.is_some_and(|s| s.step) {
                            continue;
                        }
                        shown = !shown;
                        asks += 1;
                        let (lane, back) = (lane.clone(), back.clone());
                        let changes = switched(base, shown);
                        let late = u * (rng(&mut seed) % 3) as u32 / 4;
                        std::thread::spawn(move || {
                            let r = lane.ask(Ask::Save(changes));
                            std::thread::sleep(late);
                            let _ = back.send(Ev::Answered(Box::new(r)));
                        });
                    }
                    Ev::Answered(r) => {
                        asks -= 1;
                        if let Ok(s) = &*r {
                            base = s.obs.setup.elevated;
                        }
                        if let (0, Some(st)) = (asks, status) {
                            (shown, base) = (st.on, st.on);
                        }
                    }
                    Ev::Seen(reply) => {
                        let _ = reply.send((shown, asks));
                    }
                }
            }
        });
        tx
    }

    fn seen(page: &mpsc::Sender<Ev>) -> (bool, u32) {
        let (tx, rx) = mpsc::channel();
        page.send(Ev::Seen(tx)).unwrap();
        rx.recv().unwrap()
    }

    fn settled(lane: &Lane, u: Duration) {
        let until = Instant::now() + Duration::from_secs(20);
        while !lane.idle() {
            assert!(Instant::now() < until, "the lane never went idle");
            std::thread::sleep(u);
        }
        std::thread::sleep(u * 2);
    }

    fn new_lane(world: &Arc<World>) -> Arc<Lane> {
        let lane = Arc::new(Lane::new(world.u * 3));
        let l = lane.clone();
        let _ = world.waiting.set(Box::new(move || l.queued()));
        let _ = world.lane.set(lane.clone());
        lane.start(world.clone());
        lane
    }

    #[derive(Default)]
    struct Tickets {
        m: Mutex<(u64, u64)>,
        cv: Condvar,
    }

    fn before(world: &Arc<World>, t: &Tickets, want: bool) {
        let ticket = {
            let mut g = t.m.lock().unwrap();
            let n = g.0;
            g.0 += 1;
            while g.1 != n {
                g = t.cv.wait(g).unwrap();
            }
            n
        };
        let old = world.saved();
        let mut new = old.clone();
        new.obs.setup.elevated = want;
        let mut held = None;
        match plan(&old, &new, world.copy_there()) {
            Some(_) if t.m.lock().unwrap().0 > ticket + 1 => {}
            Some(step) => {
                held = world.hold();
                let done = world.step(Job::Save, step, &old, &new, &|| {});
                world.settle();
                new = after(step, &old, new, done, world.copy_there()).0;
                let _ = world.save(&old, new);
            }
            None => {
                let _ = world.save(&old, new);
            }
        }
        drop(held);
        let mut g = t.m.lock().unwrap();
        g.1 += 1;
        t.cv.notify_all();
    }

    fn before_lane(world: &Arc<World>) -> Arc<Tickets> {
        let t = Arc::new(Tickets::default());
        let tt = t.clone();
        let _ = world.waiting.set(Box::new(move || {
            let g = tt.m.lock().unwrap();
            (g.0 - g.1).saturating_sub(1) as usize
        }));
        t
    }

    fn remove_then_on_off_on(u: Duration, ask: impl Fn(bool) + Sync) {
        std::thread::scope(|s| {
            ask(false);
            std::thread::sleep(u * 9 / 10);
            s.spawn(|| ask(true));
            std::thread::sleep(u);
            s.spawn(|| ask(false));
            std::thread::sleep(u);
            s.spawn(|| ask(true));
        });
    }

    #[test]
    fn remove_then_on_off_on_without_the_lane_leaves_a_copy_said_to_be_left() {
        let u = Duration::from_millis(40);
        let world = World::new(u, true, Instant::now() - u * 50);
        let t = before_lane(&world);
        remove_then_on_off_on(u, |want| before(&world, &t, want));
        let w = world.lock();
        assert!(on(&w.saved) && w.copy && w.task, "on, with its copy");
        assert!(
            w.left,
            "the check, run inside the Install against the setting saved before it, says a copy is left: the switch on and Remove it beside it"
        );
        assert_eq!(
            w.slow_closes, 1,
            "OBS the Remove let start, asked to close while starting"
        );
        assert!(
            w.starts_while_waiting >= 1,
            "OBS started while requests waited"
        );
        assert!(w.checks_mid >= 1, "checked while requests waited");
    }

    #[test]
    fn remove_then_on_off_on_ends_on_with_its_copy_one_prompt_each_and_obs_started_once() {
        let u = Duration::from_millis(40);
        let world = World::new(u, true, Instant::now() - u * 50);
        let lane = new_lane(&world);
        let page = page(lane.clone(), &world, 7);
        settled(&lane, u);
        // The page holds its switch while a request waits: the later clicks come from elsewhere.
        let elsewhere = |want: bool| {
            let _ = lane.ask(Ask::Save(switched(!want, want)));
        };
        page.send(Ev::Click).unwrap();
        settled_after_answer(&page, u);
        std::thread::sleep(u * 9 / 10);
        std::thread::scope(|s| {
            page.send(Ev::Click).unwrap();
            std::thread::sleep(u);
            s.spawn(|| elsewhere(false));
            std::thread::sleep(u);
            s.spawn(|| elsewhere(true));
        });
        settled_after_answer(&page, u);
        settled(&lane, u);
        let w = world.lock();
        assert!(
            on(&w.saved) && w.copy && w.task,
            "on, with its copy and its task"
        );
        assert!(!w.left, "nothing said to be left");
        assert_eq!(w.prompts, 2, "the Remove, then one Install for on, off, on");
        assert_eq!(w.slow_closes, 0, "OBS stayed stopped between the steps");
        assert_eq!(w.starts_while_waiting, 0);
        assert_eq!(w.starts, 1, "OBS started once, after the last step");
        assert!(
            matches!(w.obs, Some((true, _))),
            "with the rights, as the setting says"
        );
        assert_eq!(w.checks_mid, 0, "checked only once nothing waited");
        assert_eq!(w.holds.0, w.holds.1);
        drop(w);
        assert_eq!(
            seen(&page),
            (true, 0),
            "the page's switch shows what is saved"
        );
    }

    fn settled_after_answer(page: &mpsc::Sender<Ev>, u: Duration) {
        while seen(page).1 > 0 {
            std::thread::sleep(u / 4);
        }
    }

    #[test]
    fn on_then_off_while_obs_stops_asks_windows_nothing() {
        let u = Duration::from_millis(40);
        let clicks = |ask: &(dyn Fn(bool) + Sync)| {
            std::thread::scope(|s| {
                s.spawn(|| ask(true));
                std::thread::sleep(u);
                s.spawn(|| ask(false));
            })
        };
        let world = World::new(u, false, Instant::now());
        let t = before_lane(&world);
        clicks(&|want| before(&world, &t, want));
        assert_eq!(
            world.lock().prompts,
            2,
            "without the lane, the older on set up the copy and the newer off removed it"
        );
        let world = World::new(u, false, Instant::now());
        let lane = new_lane(&world);
        clicks(&|want| {
            let _ = lane.ask(Ask::Save(switched(!want, want)));
        });
        settled(&lane, u);
        let w = world.lock();
        assert_eq!(w.prompts, 0, "off came before Windows asked: nothing to do");
        assert!(!on(&w.saved) && !w.copy && !w.task && !w.left);
        assert!(
            matches!(w.obs, Some((false, _))),
            "the own OBS, started once"
        );
        assert_eq!(w.starts, 1);
    }

    #[test]
    fn a_hammered_switch_ends_with_the_page_the_setting_the_copy_and_obs_agreeing() {
        let u = Duration::from_millis(5);
        for round in 1..=5u64 {
            let mut seed = round * 7919 + 1;
            let world = World::new(u, round % 2 == 0, Instant::now() - u * 50);
            {
                let mut w = world.lock();
                for _ in 0..60 {
                    w.replies.push_back(match rng(&mut seed) % 20 {
                        0..=11 => Reply::Yes,
                        12..=15 => Reply::No,
                        16..=17 => Reply::Fails,
                        _ => Reply::FailsHalfway,
                    });
                }
            }
            let lane = new_lane(&world);
            let page = page(lane.clone(), &world, seed);
            std::thread::scope(|s| {
                for t in 0..3u64 {
                    let (lane, world, page) = (&lane, &world, &page);
                    s.spawn(move || {
                        let mut seed = round * 104_729 + t * 31 + 1;
                        for _ in 0..12 {
                            std::thread::sleep(u * (rng(&mut seed) % 4) as u32);
                            if t == 0 || !rng(&mut seed).is_multiple_of(3) {
                                page.send(Ev::Click).unwrap();
                            } else {
                                let saw = world.lock().saved.obs.setup.elevated;
                                let base = if rng(&mut seed).is_multiple_of(5) {
                                    !saw
                                } else {
                                    saw
                                };
                                let _ = lane.ask(Ask::Save(switched(base, !base)));
                            }
                        }
                    });
                }
            });
            settled_after_answer(&page, u);
            settled(&lane, u);
            world.lock().replies.clear();
            let newest = !seen(&page).0;
            page.send(Ev::Click).unwrap();
            settled_after_answer(&page, u);
            settled(&lane, u);
            let w = world.lock();
            let (shown, asks) = seen(&page);
            assert_eq!(asks, 0);
            assert_eq!(w.most_inside, 1, "round {round}: one step at a time");
            assert_eq!(w.holds.0, w.holds.1, "round {round}: each hold let go");
            assert_eq!(w.held, 0);
            assert_eq!(w.two_obs, 0, "round {round}: one OBS");
            assert_eq!(
                w.starts_while_waiting, 0,
                "round {round}: no start between steps"
            );
            assert_eq!(w.checks_mid, 0, "round {round}: checked only when idle");
            assert_eq!(
                w.left,
                left(on(&w.saved), w.copy, w.task),
                "round {round}: the status says what is there"
            );
            assert!(
                on(&w.saved) == w.copy || w.left,
                "round {round}: the setting and the copy agree, or a copy left is said"
            );
            assert_eq!(
                w.obs.map(|(m, _)| m),
                Some(on(&w.saved)),
                "round {round}: OBS runs as the setting says"
            );
            assert_eq!(
                shown, w.saved.obs.setup.elevated,
                "round {round}: the page's switch"
            );
            assert_eq!(shown, newest, "round {round}: the newest request wins");
            assert_eq!(lane.doing(), None);
        }
    }

    #[test]
    fn changes_apply_over_what_the_steps_before_left() {
        let mut a = with(false);
        a.obs.setup.volumes.insert("Microphone".into(), -3.0);
        let mut b = a.clone();
        b.obs.setup.elevated = true;
        b.obs.setup.volumes.remove("Microphone");
        b.obs.setup.volumes.insert("Desktop audio".into(), -6.0);
        let c = Changes::between(&a, &b);
        assert!(c.switches());
        let mut later = a.clone();
        later.obs.setup.clip_seconds = 45;
        let got = c.onto(&later).unwrap();
        assert!(got.obs.setup.elevated && got.obs.setup.clip_seconds == 45);
        assert_eq!(got.obs.setup.volumes.len(), 1);
        assert_eq!(got.obs.setup.volumes.get("Desktop audio"), Some(&-6.0));
        assert!(Changes::between(&a, &a).is_empty());
        let mut d = a.clone();
        d.obs.setup.clip_seconds = 60;
        assert!(!Changes::between(&a, &d).switches());
    }

    #[test]
    fn the_step_follows_the_switch_and_the_setup() {
        assert_eq!(step_for(false, true, false, false), Some(Step::Install));
        assert_eq!(step_for(true, false, true, true), Some(Step::Remove));
        assert_eq!(step_for(true, true, true, true), Some(Step::Write));
        assert_eq!(step_for(true, true, true, false), Some(Step::Install));
        assert_eq!(step_for(true, true, false, true), None);
        assert_eq!(step_for(false, false, true, true), None);
        let mut longer = with(true);
        longer.obs.setup.clip_seconds = 90;
        assert_eq!(plan(&with(true), &longer, true), Some(Step::Write));
        let mut louder = with(true);
        louder.obs.setup.volumes.insert("Microphone".into(), 2.0);
        assert_eq!(plan(&with(true), &louder, true), None, "over the WebSocket");
        let no = || Err("declined".to_string());
        assert!(
            after(Step::Install, &with(false), with(true), Ok(()), true)
                .0
                .obs
                .setup
                .elevated
        );
        let (s, said) = after(Step::Install, &with(false), with(true), no(), true);
        assert!(!s.obs.setup.elevated && said.unwrap().contains("stayed off"));
        assert!(
            after(Step::Remove, &with(true), with(false), no(), true)
                .0
                .obs
                .setup
                .elevated
        );
        assert!(
            !after(Step::Remove, &with(true), with(false), no(), false)
                .0
                .obs
                .setup
                .elevated
        );
        let (s, _) = after(Step::Write, &with(true), longer.clone(), no(), true);
        assert_eq!(s.obs.setup.clip_seconds, with(true).obs.setup.clip_seconds);
        assert!(left(false, true, false) && left(false, false, true));
        assert!(!left(true, true, true) && !left(false, false, false));
    }

    #[test]
    fn a_request_that_goes_wrong_is_answered_and_the_next_one_runs() {
        let lane = Arc::new(Lane::new(Duration::ZERO));
        let world = World::new(
            Duration::from_millis(2),
            false,
            Instant::now() - Duration::from_secs(1),
        );
        let _ = world.lane.set(lane.clone());
        lane.start(world.clone());
        lane.post(Ask::Run(Box::new(|| panic!("the step went wrong"))));
        let (tx, rx) = mpsc::channel::<Answer>();
        lane.push(Ask::Run(Box::new(move || drop(tx))), None);
        let _ = rx.recv();
        let r = lane.ask(Ask::Save(switched(false, true)));
        assert!(r.unwrap().obs.setup.elevated);
        settled(&lane, Duration::from_millis(2));
        assert_eq!(lane.doing(), None);
    }
}
