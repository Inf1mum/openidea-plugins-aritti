create table client (
    id bigserial primary key,
    full_name varchar(100) not null,
    email varchar(160),
    legacy_code varchar(20),
    notes text
);

create table invoice (
    id bigserial primary key,
    number varchar(30) not null,
    total numeric(12, 2),
    client_id bigint not null references client (id)
);

create table invoice_line (
    id bigserial primary key,
    invoice_id bigint not null references invoice (id),
    description varchar(200),
    quantity integer not null
);
