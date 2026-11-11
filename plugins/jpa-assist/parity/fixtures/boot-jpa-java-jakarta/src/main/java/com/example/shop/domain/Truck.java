package com.example.shop.domain;

import jakarta.persistence.Entity;
import jakarta.persistence.PrimaryKeyJoinColumn;

@Entity
@PrimaryKeyJoinColumn(name = "vehicle_id")
public class Truck extends Vehicle {

    private int payloadKg;

    public int getPayloadKg() {
        return payloadKg;
    }

    public void setPayloadKg(int payloadKg) {
        this.payloadKg = payloadKg;
    }
}
