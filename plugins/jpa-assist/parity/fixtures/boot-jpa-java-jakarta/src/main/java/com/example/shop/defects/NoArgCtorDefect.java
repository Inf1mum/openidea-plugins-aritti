// expect: jpa-no-arg-constructor
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class NoArgCtorDefect {

    @Id
    private Long id;

    public NoArgCtorDefect(Long id) {
        this.id = id;
    }
}
