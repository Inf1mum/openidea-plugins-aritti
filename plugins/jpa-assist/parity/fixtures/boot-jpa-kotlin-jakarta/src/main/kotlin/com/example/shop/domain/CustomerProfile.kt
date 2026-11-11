package com.example.shop.domain

import jakarta.persistence.Entity
import jakarta.persistence.FetchType
import jakarta.persistence.Id
import jakarta.persistence.JoinColumn
import jakarta.persistence.Lob
import jakarta.persistence.MapsId
import jakarta.persistence.OneToOne

@Entity
class CustomerProfile(
    @OneToOne(fetch = FetchType.LAZY)
    @MapsId
    @JoinColumn(name = "customer_id")
    var customer: Customer,

    @Lob
    var biography: String? = null,
) {

    @Id
    var id: Long? = null
}
