-- Supabase setup for success-lib sync.
--
-- Run this in the Supabase SQL editor of your project, then use the project
-- URL (https://<ref>.supabase.co) and an API key as the sync configuration.
--
-- Notes on access control: the simplest setup is to keep row level security
-- enabled and use a service_role key from trusted devices only, or define
-- policies bound to authenticated users. The permissive policy below allows
-- any holder of the anon key to read/write all archives — fine for a
-- personal project, not for shared deployments.

create table if not exists documents (
  archive_id text not null,
  path       text not null,
  content    text not null,
  revision   bigint not null default 1,
  updated_at timestamptz not null default now(),
  primary key (archive_id, path)
);

alter table documents enable row level security;

create policy "anon full access" on documents
  for all
  using (true)
  with check (true);
