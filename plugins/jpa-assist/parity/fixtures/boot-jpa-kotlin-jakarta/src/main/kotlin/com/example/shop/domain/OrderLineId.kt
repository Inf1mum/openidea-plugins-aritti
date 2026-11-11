package com.example.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.Embeddable
import java.io.Serializable

@Embeddable
data class OrderLineId(
    @Column(name = "order_id")
    var orderId: Long? = null,

    @Column(name = "line_no")
    var lineNo: Int? = null,
) : Serializable
