-- MySQL dialect fixture for "Generate entities from SQL file..."
create table `product` (
    `id` bigint unsigned not null auto_increment,
    `sku` varchar(40) not null,
    `title` varchar(200) not null,
    `kind` enum('PHYSICAL', 'DIGITAL', 'SERVICE') not null default 'PHYSICAL',
    `price` decimal(10, 2) not null,
    `released_on` date,
    primary key (`id`),
    unique key `uk_product_sku` (`sku`)
) engine = InnoDB;

create table `review` (
    `id` bigint not null auto_increment,
    `product_id` bigint unsigned not null,
    `rating` tinyint not null,
    `body` text,
    primary key (`id`),
    key `idx_review_product` (`product_id`),
    constraint `fk_review_product` foreign key (`product_id`) references `product` (`id`)
) engine = InnoDB;

-- aggregate view: refused by design (expressions are not read)
create view `product_rating` as
    select p.id as product_id, avg(r.rating) as average_rating
    from product p
    left join review r on r.product_id = p.id
    group by p.id;

-- plain-column view: generated as an immutable entity (asks for the id column; needs Hibernate)
create view `product_titles` as
    select `id`, `sku`, `title`
    from `product`;
