package com.example.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.MappedSuperclass
import java.time.Instant

@MappedSuperclass
abstract class Auditable {

    @Column(name = "created_at", nullable = false, updatable = false)
    var createdAt: Instant = Instant.now()

    @Column(name = "updated_at")
    var updatedAt: Instant? = null
}
