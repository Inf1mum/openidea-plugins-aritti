// expect: jpa-final-entity
package com.example.billing.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public final class AuditedFinalDefect {

    @Id
    private Long id;
}
