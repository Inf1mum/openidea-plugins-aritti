package com.example.shop.service;

import org.springframework.stereotype.Service;

@Service
public class ReportService {

    private final PricingComponent pricing;

    public ReportService(PricingComponent pricing) {
        this.pricing = pricing;
    }

    public int cents(double value) {
        return pricing.roundCents(value);
    }
}
