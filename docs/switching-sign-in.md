# Switching the sign-in method

reel keeps one list of users, whatever the sign-in mode: Settings,
**Access**. A user's clips, collections, share links and paired devices
belong to that user, so they stay through a switch as long as each person
comes back in as the same user. The mode decides how reel knows who
someone is:

| Mode | reel knows you by |
|---|---|
| `local` | your name and your password |
| `proxy` | the name the proxy in front sends, in any case |
| `oidc` | your account at the identity provider, never your name |
| `single` | nothing: everyone is the user named by [Single user](configuration.md#single-user) |

The mode itself is set in the compose file (`REEL_AUTH_MODE`) and takes
effect when the container starts, as on
[the server's page](setup/server.md#3-choose-how-people-sign-in).

## What happens, pair by pair

| From | To | What happens | What to do |
|---|---|---|---|
| `local`, `proxy` or `single` | `oidc` | Everyone keeps their user, but the provider does not know which user is whose yet. Signing in with the button without a link makes a new, empty user beside the old one. | Give every user a link first: `reel users link` for yourself, **Link** on the Access page for the others. |
| `oidc` | `oidc` at another provider (a new Issuer) | The same: the old provider's accounts count for nothing at the new one. | The same links. |
| `local`, `oidc` or `single` | `proxy` | Each request is the user with the name the proxy sends. A name reel does not have makes a new user. | Rename users to the names the proxy sends, before anyone comes in. |
| `proxy`, `oidc` or `single` | `local` | Everyone keeps their user, but nobody has a password unless one was set before. | `reel passwd` for yourself, **Set password** on the Access page for the others. |
| any | `single` | Everyone who reaches reel is the user named by Single user (`you`), made as a member when there is no such user. | Set `REEL_DEV_USER` to your own name. |

What stays through every switch:

- **Passwords** stay in the database, and work again when you switch back
  to `local`.
- **Links to the provider** stay too, so switching back to `oidc` needs no
  new links.
- **Sessions**: `local` and `oidc` share them. Someone signed in before the
  switch stays signed in until the session ends or they sign out, so after
  `local` to `oidc` you are most likely still signed in. `proxy` and
  `single` have no sessions.
- **Paired devices** keep uploading: the app signs in with its own token,
  not with the sign-in mode, and stays with the user who paired it.
- **Admins** stay admins. With a [Role claim](configuration.md#role-claim),
  the provider decides at every sign-in, but the last admin stays one.

The server's log says what to run when no admin can sign in under the new
mode, and `docker exec reel reel doctor` prints the same line.

## From a proxy to an identity provider

1. At the provider, make a client for reel as in
   [An identity provider](setup/server.md#an-identity-provider-openid-connect).
2. In the compose file, set `REEL_AUTH_MODE: oidc` and add
   `REEL_OIDC_ISSUER`, `REEL_OIDC_CLIENT_ID` and the client secret. Keep
   `REEL_TRUSTED_PROXIES`: reel still reads the visitor's address from it.
   `REEL_OIDC_NAME: Authentik` puts the name people know on the sign-in button.
3. In the proxy, take the sign-in (forward auth) off reel, so nobody signs
   in twice.
4. `docker compose up -d`. The log says
   `no admin is linked to the identity provider`.
5. Before you press the sign-in button, run this with your own user's
   name (`docker exec reel reel users list` shows the names):

    ```
    docker exec reel reel users link robin
    ```

    It prints a link that works once, for 24 hours.

6. Open the link in your browser and sign in at the provider. When it
   worked, you are in your library as yourself, an admin, with everything
   that was yours.
7. Under Settings, **Access**, every other user says **not linked**. For
   each, press **Link**, then **Copy the link**, and send it to that
   person. When they open it and sign in at the provider, that account is
   their user here.

When everyone is in, no user says **not linked** any more. Someone new,
who had no user before, signs in with the button and gets a new user as
before.

## From reel's own accounts to an identity provider

Steps 1, 2 and 4 above. You are most likely still signed in, so under
Settings, **Access**, press **Link** on your own row and open the link
yourself, then **Link** for everyone else as in step 7. If your session
has ended, use `reel users link` as in step 5.

Passwords stay, unused, for a way back.

## From a proxy or a provider to reel's own accounts

1. In the compose file, set `REEL_AUTH_MODE: local`, or take the line out:
   `local` is the default. Behind a proxy, take its sign-in off reel.
2. `docker compose up -d`. The log says `no admin has a password`.
3. Give yourself one:

    ```
    docker exec -e REEL_PASSWORD='a long password' reel reel passwd robin
    ```

4. Sign in with your name and that password. Under Settings, **Access**,
   press **Set password** for each other user and tell them.

From a provider, you can set the passwords before the switch:
**Set password** is on the Access page under `oidc` too.

## To a proxy

The proxy names each person, so reel's names have to be the proxy's.
Before the switch, under Settings, **Access**, press **Rename** on each
user whose name the proxy spells differently, and put yours in
[Admins](configuration.md#admins) to be safe. Then set the proxy up as in
[A sign-in in front](setup/server.md#a-sign-in-in-front-authentik-authelia-oauth2-proxy),
the five open paths included: the paired apps need `/api/device/`.

Behind a proxy you cannot rename yourself, since the proxy would then
bring you in as someone new;
`docker exec reel reel users rename <name> <new name>` still can.

## Back again

Switching back keeps everyone, with nothing to do: passwords and links to
the provider stayed, and a proxy sends the same names as before. Only
the people who first came in under the mode in between have no password,
no link or not the proxy's name yet; sort them out as above.

## Someone signed in before their link

They got a new, empty user, such as `mira 2`, and their link says that
their account at the provider "is the user mira 2 here already".
Under Settings, **Access**, press **Unlink** on `mira 2`, or run
`docker exec reel reel users unlink "mira 2"`. Their link works then.
`mira 2` stays, with whatever was uploaded to it, and nobody signs in as
it any more: disable it on the Access page. reel never moves an account
at the provider off one user onto another by itself; only an admin's
**Unlink** does.

## The commands

```
docker exec reel reel users list
docker exec reel reel users link <name>
docker exec reel reel users unlink <name>
docker exec reel reel users rename <name> <new name>
docker exec -e REEL_PASSWORD='...' reel reel passwd <name>
```

`list` shows every user with their role, whether they are disabled and how
they sign in under the mode in force. `link` prints a new link and
replaces one that waited; it refuses a user who is linked already. `unlink`
takes a user's account at the provider away and signs them out
everywhere. reel never matches a provider's account to a user by name or
email by itself: a link is the only way across.
