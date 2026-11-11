package com.example.shop.domain;

import jakarta.persistence.PrePersist;
import jakarta.persistence.PreUpdate;

public class OrderListener {

    @PrePersist
    public void beforeInsert(Object entity) {
    }

    @PreUpdate
    public void beforeUpdate(Object entity) {
    }
}
