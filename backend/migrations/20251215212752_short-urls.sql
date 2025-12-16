CREATE TABLE short_urls (
    id INTEGER NOT NULL PRIMARY KEY,
    bible_references BLOB NOT NULL
);
CREATE INDEX short_urls_by_references ON short_urls (bible_references);
