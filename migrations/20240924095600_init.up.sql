-- Add up migration script here
CREATE TABLE transactions (
    id char(64) PRIMARY KEY,
    data bytea not null
);