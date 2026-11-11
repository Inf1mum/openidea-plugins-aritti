create table customers (id bigint not null, email varchar(255), name varchar(80) not null, department_id bigint not null, primary key (id)) engine=InnoDB;
create table customers_SEQ (next_val bigint) engine=InnoDB;
insert into customers_SEQ values ( 1 );
create table departments (id bigint not null, title varchar(60) not null, primary key (id)) engine=InnoDB;
alter table customers add constraint uq_customer_email unique (email);
alter table customers add constraint FKdiyv3bkax4ggg3m3yrbqntd0j foreign key (department_id) references departments (id);
