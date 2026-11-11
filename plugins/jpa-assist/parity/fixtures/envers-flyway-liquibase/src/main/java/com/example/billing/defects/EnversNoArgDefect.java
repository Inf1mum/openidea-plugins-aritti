// expect: jpa-no-arg-constructor, jpa-column-precision-type
package com.example.billing.defects;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class EnversNoArgDefect {

    @Id
    private Long id;

    @Column(precision = 5)
    private String code;

    public EnversNoArgDefect(Long id) {
        this.id = id;
    }
}
