create type url_uploads as enum ('disabled', 'sso_only', 'everyone');

alter table elysia_settings
  add column url_uploads url_uploads not null default 'everyone';

update elysia_settings set url_uploads = 'disabled' where not url_uploads_enabled;

alter table elysia_settings drop column url_uploads_enabled;

comment on column elysia_settings.url_uploads is 'Who may rehost images from a URL: nobody, only the SSO provider uploading on behalf of its users, or everyone';
