-- PostgreSQL dialect fixture for "Generate entities from SQL file..."
create type order_state as enum ('NEW', 'PAID', 'SHIPPED', 'CANCELLED');

create table customers (
    id bigserial primary key,
    email varchar(160) not null unique,
    display_name varchar(100) not null,
    created_at timestamp not null,
    credit_limit numeric(12, 2)
);

comment on table customers is 'People who buy things';
comment on column customers.email is 'Login e-mail address';

create table orders (
    id bigserial primary key,
    customer_id bigint not null references customers (id),
    state order_state not null,
    placed_on date,
    note text
);

create index idx_orders_customer on orders (customer_id);

create table order_items (
    order_id bigint not null references orders (id),
    line_no integer not null,
    sku varchar(40) not null,
    quantity integer not null,
    primary key (order_id, line_no)
);

create table premium_customers (
    id bigint primary key references customers (id),
    tier varchar(10) not null
);

-- aggregate view: refused by design (expressions are not read)
create view customer_orders as
    select c.id as customer_id, c.display_name, count(o.id) as order_count
    from customers c
    left join orders o on o.customer_id = c.id
    group by c.id, c.display_name;

-- plain-column view: generated as an immutable entity (asks for the id column; needs Hibernate)
create view customer_names as
    select id, email, display_name
    from customers;
