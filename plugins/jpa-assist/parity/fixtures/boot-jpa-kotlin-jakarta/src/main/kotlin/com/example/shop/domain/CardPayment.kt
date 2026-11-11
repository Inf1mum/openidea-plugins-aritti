package com.example.shop.domain

import jakarta.persistence.DiscriminatorValue
import jakarta.persistence.Entity

@Entity
@DiscriminatorValue("CARD")
class CardPayment : Payment() {

    var last4: String? = null
}
