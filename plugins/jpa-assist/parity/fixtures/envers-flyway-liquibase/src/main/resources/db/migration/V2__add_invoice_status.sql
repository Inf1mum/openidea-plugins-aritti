alter table invoice add column status varchar(16) not null default 'DRAFT';

create index idx_invoice_client on invoice (client_id);
