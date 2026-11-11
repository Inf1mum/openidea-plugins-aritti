// expect: jpa-listener-constructor (private no-arg ctor), jpa-callback-modifiers (static), entity-listener-signature (zero params), jpa-duplicate-lifecycle-event
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.EntityListeners;
import jakarta.persistence.Id;
import jakarta.persistence.PostLoad;
import jakarta.persistence.PrePersist;

@Entity
@EntityListeners(NestedListenerDefects.Audit.class)
public class NestedListenerDefects {

    @Id
    private Long id;

    public static class Audit {

        private Audit() {
        }

        @PrePersist
        public void first(NestedListenerDefects entity) {
        }

        @PrePersist
        public void second(NestedListenerDefects entity) {
        }

        @PostLoad
        public static void staticCallback(NestedListenerDefects entity) {
        }

        @PostLoad
        public void noParameter() {
        }
    }
}
