// expect: jpa-callback-modifiers (static), entity-listener-signature (return type / parameter), jpa-duplicate-lifecycle-event (second @PrePersist)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.PostLoad;
import jakarta.persistence.PrePersist;

@Entity
public class EntityCallbackDefects {

    @Id
    private Long id;

    @PrePersist
    void first() {
    }

    @PrePersist
    void second() {
    }

    @PostLoad
    public static void staticCallback() {
    }

    @PostLoad
    public int wrongReturn() {
        return 0;
    }
}
