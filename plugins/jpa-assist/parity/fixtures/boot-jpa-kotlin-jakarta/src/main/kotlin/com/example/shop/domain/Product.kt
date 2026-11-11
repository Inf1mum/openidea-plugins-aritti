package com.example.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.Entity
import jakarta.persistence.GeneratedValue
import jakarta.persistence.GenerationType
import jakarta.persistence.Id
import jakarta.persistence.ManyToMany
import jakarta.persistence.NamedQuery
import java.math.BigDecimal

@Entity
@NamedQuery(name = "Product.findCheap", query = "select p from Product p where p.price < :limit")
class Product(
    @Column(nullable = false, length = 120)
    var name: String,

    @Column(precision = 10, scale = 2)
    var price: BigDecimal? = null,
) : Auditable() {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    var id: Long? = null

    @ManyToMany(mappedBy = "products")
    var categories: MutableSet<Category> = mutableSetOf()
}
