create table if not exists users (
  id string primary key not null,
  username text not null unique,
  password_hash text not null
);
