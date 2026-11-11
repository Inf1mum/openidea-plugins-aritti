package com.example.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.Embeddable

@Embeddable
class Address(
    @Column(length = 120)
    var street: String? = null,

    @Column(length = 80)
    var city: String? = null,

    @Column(name = "postal_code", length = 12)
    var postalCode: String? = null,
)
