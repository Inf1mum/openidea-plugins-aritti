package com.example.shop.service

import org.springframework.stereotype.Component

@Component
class PricingComponent {

    fun roundCents(value: Double): Int = Math.round(value * 100).toInt()
}
