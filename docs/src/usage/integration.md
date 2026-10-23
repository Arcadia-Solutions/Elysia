# Integrating with another site

Elysia is designed to serve images embedded in another site. The pieces below
are independent, use the ones you need.

## Public URL

Set `server.public_url` to the address Elysia is reached at from the outside.
The API then returns absolute image links, which keep working once stored on
another site. Set it before the other site stores any link: links already
stored keep whatever was returned at the time.

## Single sign-on

With an `auth.sso` block in `config.yml`, the login page offers "Login with
&lt;name&gt;" using OpenID Connect. Register Elysia as a client at the provider with
the frontend callback page (`https://<elysia>/auth/sso/callback`) as redirect
URI, and set the same value as `redirect_uri`. Accounts are created on first
login.

The optional `status_endpoint` is not part of OpenID Connect. When set, Elysia
asks it whether the user is still active and ends their session within a minute
of the provider banning them, instead of waiting for the session to expire. Arcadia
provides one.

Disable open registration in Settings if every user should come through the
provider.

## Uploading on behalf of users

The SSO provider's backend can upload for its users without them visiting
Elysia: call `POST /api/upload` (file) or `POST /api/upload-url` (link) with:

- the provider's client id and secret as HTTP Basic auth,
- `On-Behalf-Of-Sub`: the user's subject at the provider,
- `On-Behalf-Of-Username`: the username to create the account with, if it does
  not exist yet.

The upload counts against that user's quota as if they made it.

## API keys

Any user can create API keys from the Account page, to upload from scripts or
other tools. A key is sent as `Authorization: Bearer <token>`. It has one of
two scopes:

- **Upload**: only the upload routes.
- **Full access**: everything the user can do, except changing their password,
  managing API keys and admin actions.

The token is shown only once. Keys can expire and be revoked at any time.

## API reference

The backend serves its OpenAPI description at `/api-docs/openapi.json` and a
Swagger UI at `/swagger-ui/`.

## Hotlink protection

Fill the allowed referrers in Settings with the embedding site's hostnames so
other sites cannot hotlink your images, see
[Administration](administration.md#allowed-referrers).
