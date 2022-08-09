CREATE TABLE IF NOT EXISTS publish_keys (
    pw varchar(128) NOT NULL,
    user varchar(128) NOT NULL,
    
    UNIQUE(pw, user)
)
