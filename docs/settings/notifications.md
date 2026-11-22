# Notifications

Where reel sends a note when something needs you, and for which events: a failed job, a file it
cannot read, a disk running out of space, trouble with the external storage, clips the library
cap moved, a new release, a new link. A note goes to an ntfy topic or to a webhook of your own.
These settings are kept on the server, the same for every PC and browser. A setting given in
`config.toml` or the environment shows read only here, with a padlock and where it came from.
What a note carries, and where it goes: [What your server contacts](../privacy.md#what-your-server-contacts).

The app's standalone mode has no Notifications page: reel on one PC sends no notes, and the
page's address answers "Not in standalone".

## Where {#where}

### Send to {#notify-kind}

Where the notes go: **off** (the default), **an ntfy topic** or **a webhook**. While it is off,
reel sends nothing, whatever the events below say.

Fill in the rows below first: reel refuses **an ntfy topic** while **ntfy server** or **ntfy
topic** is empty, and **a webhook** while **Webhook URL** is empty. Then try it with
[Send a test note](#notify-test). reel waits 10 seconds for an answer and does not send a failed
note again; the server's log says why it failed.

In `config.toml`: `kind` under `[notify]`; environment: `REEL_NOTIFY_KIND`; every value it
takes: [Configuration](../configuration.md#send-to).

### ntfy server {#notify-ntfy-url}

The address of the ntfy server, such as `https://ntfy.example.com`: the server itself, without
the topic. reel posts each note to it with the topic, a title, the message, a link that opens
when you tap the note, and a tag ntfy shows as an icon.

It takes https, and plain http only for a host on your own network: a loopback or private
address, a name without a dot, or one ending in `.local`, `.lan`, `.internal` or `.home.arpa`.

A new address clears the **ntfy token** saved under Settings, so a token written for one server
never goes to another: enter it again for the new one. When the token is set in `config.toml` or
the environment, reel refuses a new address here: change both there.

In `config.toml`: `ntfy_url` under `[notify]`; environment: `REEL_NTFY_URL`; every value it
takes: [Configuration](../configuration.md#ntfy-server).

### ntfy topic {#notify-ntfy-topic}

The topic reel posts to. Every phone or browser subscribed to that topic on that server gets the
notes. For a protected topic, set the **ntfy token** too.

In `config.toml`: `ntfy_topic` under `[notify]`; environment: `REEL_NTFY_TOPIC`; every value it
takes: [Configuration](../configuration.md#ntfy-topic).

### ntfy token {#notify-ntfy-token}

An access token for a protected topic, sent with every note as a bearer token. It is written,
never shown again: the row says only whether one is set. A new **ntfy server** clears it, as
above. The exports of the database and of the settings leave it blank, so after a restore from
one, enter it again ([Restore a server](../backup.md#restore-a-server)).

In `config.toml`: `ntfy_token` under `[notify]`; environment: `REEL_NTFY_TOKEN`; every value it
takes: [Configuration](../configuration.md#ntfy-token).

### Webhook URL {#notify-webhook-url}

reel posts each note to this address as JSON, such as:

```json
{
  "event": "link_made",
  "title": "reel: a link was made",
  "body": "Clutch round by robin",
  "link": "https://clips.example.com/s/8xQvT1n4wKpR9sLm2cHd7A",
  "site": "reel",
  "at": "2026-11-22T21:04:11.482917305Z"
}
```

- `event`: the event's code, as listed under [Events](#notify-events), or `test` for
  [Send a test note](#notify-test).
- `title`: the **Site name**, then what happened.
- `body`: the details, such as the clip and the reason.
- `link`: the page to look at: the failed jobs, the Status page, the trash, the release notes or
  the new link.
- `site`: the **Site name**.
- `at`: the time of sending, in UTC (RFC 3339).

The webhook has to answer with a success status (2xx); any other answer counts as a failure. The
address takes https, and plain http only for a host on your own network, as for the
**ntfy server**.

In `config.toml`: `webhook_url` under `[notify]`; environment: `REEL_WEBHOOK_URL`; every value
it takes: [Configuration](../configuration.md#webhook-url).

## Events {#events}

### Events {#notify-events}

What reel tells you about. Each event is a row with its own switch. The first five are on for a
new install; **An update is available** and **A link was made** are off. **Reset** puts the
first five back. An event's note goes out only while **Send to** is not off.

- **A job failed** (`job_failed`): an ingest, a transcode or a cut stopped with an error. The
  note names the job, the clip and the reason, with reel's folders written as their names, and
  links to the Jobs page's failed jobs.
- **A file could not be read** (`file_unreadable`): a file in the incoming folder that ffmpeg
  cannot read, moved to the incoming folder's `failed` folder. The note names the file and the
  reason ([A clip is in the incoming folder's failed folder](../troubleshooting.md#a-clip-is-in-the-incoming-folders-failed-folder)).
- **The disk is low** (`disk_low`): a disk that holds reel's data falls under the
  [Disk warning](storage.md#storage-disk-warn-gb) (Library and storage). reel looks every ten
  minutes, sends one note for that disk, and none again until it has been above the warning. The
  note names the disks and links to the Status page.
- **The external storage needs a look** (`storage`): the link previews' storage reaches 80 % or
  100 % of its [Limit](sharing.md#external-limit-gb), or an upload or a delete there failed
  ([When the storage cannot be reached](../storage.md#when-the-storage-cannot-be-reached)).
- **The library cap moved clips** (`library_cap`): once each time the
  [Library cap](storage.md#storage-cap-gb) moves clips to the trash, with how many and their
  size. The note links to the trash, where you can restore them for the trash's days
  ([The library cap](../sharing.md#the-library-cap)).
- **An update is available** (`update_available`): a newer reel is out. It needs
  [Check for updates](advanced.md#advanced-update-check) on, and comes with each check that finds
  the release: once a day until you update, and on the Status page's **Check now**. The note
  links to the release notes.
- **A link was made** (`link_made`): a new share link, made on the pages or from the app. The
  note names the clip and who made it (and the device, from the app), and links to the link.
  Links made for several picked clips at once come as one note, with how many.

In `config.toml`: `events` under `[notify]`; environment: `REEL_NOTIFY_EVENTS`; every value it
takes: [Configuration](../configuration.md#events).

## Try it {#try-it}

### Send a test note {#notify-test}

**Send a test** sends a note at once to where the rows under **Where** point, whatever the
events say. Its title is the **Site name** followed by "a test", and it says who sent it. The
answer shows under the row: where it went ("sent to the ntfy topic" and the topic, or "the
webhook answered" and the status), or why it did not go, such as notifications being off, a
missing server or topic, a server that cannot be reached, or the status a refused token or a
webhook answered. Use it after each change under **Where**.
