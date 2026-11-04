//! The shape of reel's Settings pages (sections, user, health, jobs) for the app's own section.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shape {
    #[serde(default)]
    pub sections: Vec<ShapeSection>,
    #[serde(default)]
    pub who: ShapeWho,
    #[serde(default)]
    pub health: ShapeHealth,
    #[serde(default)]
    pub jobs: ShapeJobs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeSection {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub line: String,
    #[serde(default)]
    pub changed: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeWho {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub line: String,
    #[serde(default)]
    pub admin: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeHealth {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub line: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeJobs {
    #[serde(default)]
    pub queued: u64,
    #[serde(default)]
    pub running: u64,
    #[serde(default)]
    pub failed: u64,
}

const MOST_SECTIONS: usize = 32;

fn clean(s: &str, most: usize) -> Option<String> {
    let s = s.trim();
    if s.chars().any(char::is_control) || s.chars().count() > most {
        return None;
    }
    Some(s.to_string())
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

impl Shape {
    /// A shape from a page or a server, taken whole within bounds or not at all.
    pub fn checked(v: Value) -> Option<Shape> {
        let shape: Shape = serde_json::from_value(v).ok()?;
        if shape.sections.len() > MOST_SECTIONS {
            return None;
        }
        let mut sections = Vec::with_capacity(shape.sections.len());
        for s in shape.sections {
            if !id_ok(&s.id) || s.changed > 9999 {
                return None;
            }
            sections.push(ShapeSection {
                id: s.id,
                label: clean(&s.label, 48).filter(|l| !l.is_empty())?,
                line: clean(&s.line, 240)?,
                changed: s.changed,
            });
        }
        let state = match shape.health.state.as_str() {
            "ok" | "warn" | "err" => shape.health.state,
            "" => "ok".to_string(),
            _ => return None,
        };
        if shape.jobs.queued > 99_999 || shape.jobs.running > 99_999 || shape.jobs.failed > 99_999 {
            return None;
        }
        Some(Shape {
            sections,
            who: ShapeWho {
                name: clean(&shape.who.name, 60)?,
                line: clean(&shape.who.line, 80)?,
                admin: shape.who.admin,
            },
            health: ShapeHealth {
                state,
                line: clean(&shape.health.line, 200)?,
            },
            jobs: shape.jobs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_servers_shape_passes_as_it_is() {
        let v = json!({
            "sections": [{"id": "general", "label": "General", "line": "The name.", "changed": 2}],
            "who": {"name": "robin", "line": "admin", "admin": true},
            "health": {"state": "warn", "line": "Status: the disk is 91% full"},
            "jobs": {"queued": 1, "running": 2, "failed": 0}
        });
        let s = Shape::checked(v).expect("a good shape");
        assert_eq!(s.sections[0].id, "general");
        assert_eq!(s.sections[0].changed, 2);
        assert_eq!(s.who.name, "robin");
        assert_eq!(s.health.state, "warn");
        assert_eq!(s.jobs.running, 2);
    }

    #[test]
    fn a_pages_shape_may_leave_things_out() {
        let v =
            json!({ "sections": [{"id": "devices", "label": " Devices "}], "who": {"name": "me"} });
        let s = Shape::checked(v).expect("defaults fill the rest");
        assert_eq!(s.sections[0].label, "Devices");
        assert_eq!(s.health.state, "ok");
        assert_eq!(s.jobs, ShapeJobs::default());
    }

    #[test]
    fn what_is_out_of_bounds_is_refused_whole() {
        let bad = [
            json!({ "sections": [{"id": "../x", "label": "Up"}] }),
            json!({ "sections": [{"id": "general", "label": ""}] }),
            json!({ "sections": [{"id": "general", "label": "a\u{0007}b"}] }),
            json!({ "sections": [{"id": "general", "label": "x".repeat(49)}] }),
            json!({ "health": {"state": "fine"} }),
            json!({ "who": {"name": "x".repeat(61)} }),
            json!({ "jobs": {"failed": 100000} }),
            json!({ "sections": 5 }),
        ];
        for v in bad {
            assert!(Shape::checked(v.clone()).is_none(), "{v}");
        }
        let many: Vec<Value> = (0..33)
            .map(|i| json!({"id": format!("s{i}"), "label": "S"}))
            .collect();
        assert!(Shape::checked(json!({ "sections": many })).is_none());
    }
}
