# Access

Who gets in, and how reel knows who they are: the sign-in mode and what it needs, how long a
session lasts, an identity provider's details, the users and their roles, and the failed
sign-ins. Share links stay public whatever is set here. These settings are kept on the server,
the same for every PC and browser. A setting given in `config.toml` or the environment shows read
only here, with a padlock and where it came from; **Mode**, **Trusted proxies** and **Single
user** are always read only, as reel needs them before its database opens. A setting marked
**after a restart** takes effect at the next start ([Restart reel](advanced.md#restart)).
Choosing a mode: [Choose how people sign in](../setup/server.md#3-choose-how-people-sign-in);
what each one guards: [Who can sign in](../privacy.md#who-can-sign-in); changing it on a server
with users: [Switching the sign-in method](../switching-sign-in.md).

The app's standalone mode has no Access page: reel on that PC has one user, the person at the
PC, and no sign-in, and the page's address answers "Not in standalone".

## Identity {#identity}

### Mode {#auth-mode}

How people sign in:

- **local accounts** (`local`, the default): reel's own names and passwords. The first admin
  signs up with the setup code from the server's log, and admins add everyone else under
  [Users](#users).
- **a proxy in front** (`proxy`): a sign-in in front of reel (Authentik, Authelia,
  oauth2-proxy) names the user in the `X-authentik-username` header, and reel believes it only
  from the [Trusted proxies](#auth-trusted-proxies).
- **an identity provider** (`oidc`): reel sends people to an OpenID Connect provider and knows
  each user by the provider's issuer and subject. It needs the [Issuer](#auth-oidc-issuer), the
  [Client id](#auth-oidc-client-id) and the client secret.
- **single user** (`single`): no sign-in; everyone who reaches reel is the
  [Single user](#auth-dev-user). reel refuses to start with it on anything but a loopback
  address.

reel reads it when it starts, from `config.toml` or the environment only. Before you change it on
a server with users, read [Switching the sign-in method](../switching-sign-in.md): it says who
keeps their user and what to do.

In `config.toml`: `mode` under `[auth]`; environment: `REEL_AUTH_MODE`; every value it takes:
[Configuration](../configuration.md#mode).

### Trusted proxies {#auth-trusted-proxies}

The addresses of the proxies in front of reel, each as a network, such as `10.0.0.2/32` for one
address; reel refuses to start with an entry that is not one. Only a request from one of them:

- may name the user, in proxy mode. A request from anywhere else gets a 401 page, whatever its
  headers say, and reel refuses to start in proxy mode with this list empty.
- has its visitor's address read from `X-Forwarded-For`, in every mode. That address is what the
  sign-in limits count and what [Sign-in attempts](#sign-in-attempts) shows; without the proxy
  in this list, every visitor has the proxy's address.

List the proxy's exact address, never a whole network. reel reads it when it starts, from
`config.toml` or the environment only ([Behind a reverse proxy](../privacy.md#behind-a-reverse-proxy)).

In `config.toml`: `trusted_proxies` under `[auth]`; environment: `REEL_TRUSTED_PROXIES`; every
value it takes: [Configuration](../configuration.md#trusted-proxies).

### Single user {#auth-dev-user}

The name everyone is in single mode, `you` by default; no other mode reads it. reel makes that
user when it first sees the name: an admin when it is the first user or listed in
[Admins](#auth-admins), a member otherwise. When you switch a server with users to single, set
it to your own name, so you come in as yourself
([What happens, pair by pair](../switching-sign-in.md#what-happens-pair-by-pair)). reel reads it
when it starts, from `config.toml` or the environment only.

In `config.toml`: `dev_user` under `[auth]`; environment: `REEL_DEV_USER`; every value it takes:
[Configuration](../configuration.md#single-user).

### Admins {#auth-admins}

Names that are admins, each exactly as reel gets it, case included. What a name is, and when the
list counts, depends on the mode:

- **Behind a proxy and in single mode**: the name the proxy sends, or the Single user. A listed
  user is made an admin at their next request. Taking a name off the list does not make that
  user a member again: change the role under [Users](#users). The first user reel ever sees is an
  admin too.
- **With an identity provider**: the subject the provider sends for a user, not a display name
  (Authentik's subject is the username unless its client says otherwise). The list counts at a
  user's first sign-in, and with a [Role claim](#auth-oidc-role-claim) at every sign-in, where a
  listed subject stays an admin whatever the claim says. Nobody is an admin by being first here:
  the first admin is listed, carries the [Admin value](#auth-oidc-admin-value), or types the
  setup code ([An identity provider](../setup/server.md#an-identity-provider-openid-connect)).
- **With reel's own accounts** the list makes nobody an admin: the first admin signs up with the
  setup code, and an admin sets everyone else's role under [Users](#users).

In `config.toml`: `admins` under `[auth]`; environment: `REEL_ADMINS`; every value it takes:
[Configuration](../configuration.md#admins).

## Sessions {#sessions}

A session is a browser's sign-in, with reel's own accounts or behind an identity provider;
behind a proxy and in single mode reel keeps none. The whole story:
[Passwords and sessions](../privacy.md#passwords-and-sessions).

### Idle timeout {#auth-session-days}

A session ends this long after its last use. From 1 to 365 days: 30 by default, and 7 behind an
identity provider unless you set it. It takes effect at the next start. Behind an identity
provider a short timeout matters more: the provider signs a person in again without a password,
and someone you disable there keeps the session they have until it ends
([An identity provider](../setup/server.md#an-identity-provider-openid-connect)). To end it at
once, disable the user under [Users](#users).

In `config.toml`: `session_days` under `[auth]`; environment: `REEL_SESSION_DAYS`; every value
it takes: [Configuration](../configuration.md#idle-timeout).

### Session lifetime {#auth-session-max-days}

A session ends this long after the sign-in, however active it is; 0 is no limit. From 0 to 3650
days: 90 by default, and 30 behind an identity provider unless you set it. It cannot be shorter
than the [Idle timeout](#auth-session-days), unless it is 0. A change counts at once, for the
sessions there are too: reel checks each request against the time its session signed in.

In `config.toml`: `session_max_days` under `[auth]`; environment: `REEL_SESSION_MAX_DAYS`; every
value it takes: [Configuration](../configuration.md#session-lifetime).

## Identity provider {#identity-provider}

These rows count only with **Mode** set to an identity provider (`oidc`), though they show in
every mode. The client secret is never a setting: it comes from `REEL_OIDC_CLIENT_SECRET`, or
from the file `REEL_OIDC_CLIENT_SECRET_FILE` names. Step by step:
[An identity provider](../setup/server.md#an-identity-provider-openid-connect).

### Issuer {#auth-oidc-issuer}

The provider's OpenID Connect issuer URL, exactly as the provider gives it, its last slash
included, such as `https://auth.example.com/application/o/reel/`: reel compares it character for
character. reel asks it for its metadata at start, again by itself until it answers, and every
hour after. It takes effect at the next start. Each user is bound to the issuer they signed in
through, so a new issuer is a new provider to reel, and every user needs a link again
([What happens, pair by pair](../switching-sign-in.md#what-happens-pair-by-pair)).

In `config.toml`: `oidc_issuer` under `[auth]`; environment: `REEL_OIDC_ISSUER`; every value it
takes: [Configuration](../configuration.md#issuer).

### Provider name {#auth-oidc-name}

What the sign-in page calls the provider, as in **Sign in with Authentik**; the pages' messages
about the provider use it too, as in "Authentik is not reachable yet". Empty shows the issuer's
host. It shows as typed, on one line, at most 40 characters.

In `config.toml`: `oidc_name` under `[auth]`; environment: `REEL_OIDC_NAME`; every value it
takes: [Configuration](../configuration.md#provider-name).

### Client id {#auth-oidc-client-id}

The id of the client (the application) registered for reel at the provider. Its secret comes
from `REEL_OIDC_CLIENT_SECRET` or the file `REEL_OIDC_CLIENT_SECRET_FILE` names, never from here.
The client's redirect URI is the [Public URL](general.md#public-url) followed by
`/oidc/callback`. It takes effect at the next start.

In `config.toml`: `oidc_client_id` under `[auth]`; environment: `REEL_OIDC_CLIENT_ID`; every
value it takes: [Configuration](../configuration.md#client-id).

### Scopes {#auth-oidc-scopes}

What reel asks the provider for, space separated: `openid profile email` by default. reel
always asks for `openid`, listed or not. The claims reel reads, the
[Name claim](#auth-oidc-username-claim) and the [Role claim](#auth-oidc-role-claim), come in the
ID token, and a provider may put a claim there only for a scope of its own: add that scope here.
It takes effect at the next start.

In `config.toml`: `oidc_scopes` under `[auth]`; environment: `REEL_OIDC_SCOPES`; every value it
takes: [Configuration](../configuration.md#scopes).

### Name claim {#auth-oidc-username-claim}

The claim that names a user the first time reel sees them: `preferred_username` by default, or
`email` (taken only when the provider marks it verified), `name`, or any other claim that holds
a string. When it is missing or empty, the user is named by their subject. A name is cut to 40
characters, and a name another user has gets a number, as `mira 2`. A known user keeps their
name: who a user is comes from the issuer and the subject, never from the name, and an admin
renames under [Users](#users).

In `config.toml`: `oidc_username_claim` under `[auth]`; environment: `REEL_OIDC_USERNAME_CLAIM`;
every value it takes: [Configuration](../configuration.md#name-claim).

### Role claim {#auth-oidc-role-claim}

A claim, a string or a list, such as `groups`, that makes a user an admin when it holds the
[Admin value](#auth-oidc-admin-value). Set, reel follows it at every sign-in, both ways: a user
who gains the value becomes an admin, one who loses it a member, except that the
[Admins](#auth-admins) list and the last admin stay admins. So a role changed under Users lasts
until that user's next sign-in. Empty, only the Admins list makes admins, at a user's first
sign-in.

In `config.toml`: `oidc_role_claim` under `[auth]`; environment: `REEL_OIDC_ROLE_CLAIM`; every
value it takes: [Configuration](../configuration.md#role-claim).

### Admin value {#auth-oidc-admin-value}

The value in the role claim that means admin, `admin` by default, matched exactly. It cannot be
empty while a role claim is set, or every empty claim would make an admin.

In `config.toml`: `oidc_admin_value` under `[auth]`; environment: `REEL_OIDC_ADMIN_VALUE`; every
value it takes: [Configuration](../configuration.md#admin-value).

## Users {#users}

Everyone with a user on this server. reel keeps one list whatever the sign-in mode
([Switching the sign-in method](../switching-sign-in.md)). Members see
the whole library, upload, and edit and share what they uploaded; admins do everything, Settings
included. Each row has the name ("you" beside yours), the role, how many clips are theirs, when
they were last seen, and their state: active or disabled, and with an identity provider
**not linked** for a user not yet bound to an account there (one added here, or one from before
the switch), until they sign in through a link.

- **Add user**: a name of 1 to 40 characters and a role. With reel's own accounts, also a
  password of at least 12 characters, which you pass on. In the other modes reel keeps no
  password: behind a proxy the user comes in when the proxy sends that exact name, and with an
  identity provider through **Link**. A name another user has, in any case, is refused.
- **Sign out everywhere**, with reel's own accounts or an identity provider: after a question,
  it ends every session of your own account, this browser's included, and takes you to the
  sign-in page. Paired apps stay paired: they use their own tokens.
- **Role**: member or admin, saved as you pick it. Your own row's is fixed, and the last admin
  stays an admin. With a [Role claim](#auth-oidc-role-claim), the provider decides again at the
  user's next sign-in; behind a proxy or in single mode, a name in [Admins](#auth-admins) is made
  an admin again at its next request.
- **Rename**: 1 to 40 characters, refused when another user has the name in any case. Behind a
  proxy or in single mode the name is who a person is: you cannot rename yourself there (another
  admin can, or `reel users rename`), and a user renamed to a name the proxy does not send comes
  back as a new, empty user. Before a switch to a proxy, rename users to the proxy's names
  ([To a proxy](../switching-sign-in.md#to-a-proxy)).
- **Set password**, with reel's own accounts or an identity provider: a new password of at least
  12 characters for that user, which signs them out everywhere. Behind an identity provider it is
  for a later switch to reel's own accounts
  ([From a proxy or a provider to reel's own accounts](../switching-sign-in.md#from-a-proxy-or-a-provider-to-reels-own-accounts)).
- **Link**, with an identity provider: a link for that user that works once, for 24 hours.
  Whoever opens it and signs in at the provider is that user here from then on, with everything
  that is theirs. **Copy the link** and send it to them; a new link replaces the last one, and a
  disabled user gets none. Your own row has it too while you are not linked
  ([From a proxy to an identity provider](../switching-sign-in.md#from-a-proxy-to-an-identity-provider)).
- **Unlink**, with an identity provider: after a question, it unbinds the user from their account
  at the provider and signs them out; a new link lets them in again. It is not on your own row:
  another admin unlinks you
  ([Someone signed in before their link](../switching-sign-in.md#someone-signed-in-before-their-link)).
- **Disable**, after a question, and **Enable**: a disabled user is signed out at their next
  request, cannot sign in, and their paired apps stop uploading, until enabled again. Their clips
  stay. It is not on your own row, and the last admin stays enabled.

## Sign-in attempts {#sign-in-attempts}

With reel's own accounts or an identity provider: the failed sign-ins of the last 7 days, newest
first and at most 50, each with the address, the name tried and when. A wrong setup code counts
too, with "first admin" or "setup code" as the name; behind an identity provider, where the
provider takes the passwords, those are all it lists. reel removes the tries after a week.

An address gets five tries in two minutes, then one more every two minutes, and so does a name;
[Passwords and sessions](../privacy.md#passwords-and-sessions) has the limits in full. Behind a
reverse proxy, the address is the visitor's only when the proxy is in
[Trusted proxies](#auth-trusted-proxies).
