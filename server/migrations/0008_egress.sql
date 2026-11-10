CREATE TABLE monthly_egress (
    month text PRIMARY KEY,
    bytes bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now()
);
