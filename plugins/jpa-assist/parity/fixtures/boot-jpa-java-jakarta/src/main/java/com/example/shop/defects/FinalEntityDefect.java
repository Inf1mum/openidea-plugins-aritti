// expect: jpa-final-entity
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public final class FinalEntityDefect {

    @Id
    private Long id;
}
