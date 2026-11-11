package com.example.shop.service;

import org.springframework.stereotype.Component;

@Component
public class PricingComponent {

    public int roundCents(double value) {
        return (int) Math.round(value * 100);
    }
}
