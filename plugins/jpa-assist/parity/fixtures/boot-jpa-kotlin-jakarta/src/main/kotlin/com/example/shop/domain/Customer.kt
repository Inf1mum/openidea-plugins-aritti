package com.example.shop.domain

import jakarta.persistence.CascadeType
import jakarta.persistence.Column
import jakarta.persistence.Embedded
import jakarta.persistence.Entity
import jakarta.persistence.FetchType
import jakarta.persistence.GeneratedValue
import jakarta.persistence.GenerationType
import jakarta.persistence.Id
import jakarta.persistence.NamedAttributeNode
import jakarta.persistence.NamedEntityGraph
import jakarta.persistence.OneToMany
import jakarta.persistence.OneToOne
import jakarta.persistence.Table
import jakarta.persistence.Version

@Entity
@Table(name = "customer")
@NamedEntityGraph(name = "Customer.withOrders", attributeNodes = [NamedAttributeNode("orders")])
class Customer(
    @Column(nullable = false, length = 100)
    var name: String,

    @Column(unique = true, length = 160)
    var email: String? = null,
) : Auditable() {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    var id: Long? = null

    @Version
    var version: Long = 0

    @Embedded
    var address: Address? = null

    @OneToOne(mappedBy = "customer", cascade = [CascadeType.ALL], fetch = FetchType.LAZY)
    var profile: CustomerProfile? = null

    @OneToMany(mappedBy = "customer", fetch = FetchType.LAZY)
    var orders: MutableList<CustomerOrder> = mutableListOf()
}
