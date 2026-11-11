// expect: jpa-duplicate-lifecycle-event, jpa-callback-modifiers, entity-listener-signature
package com.example.library.defects;

import javax.persistence.Entity;
import javax.persistence.Id;
import javax.persistence.PostLoad;
import javax.persistence.PrePersist;

@Entity
public class JavaxCallbackDefects {

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
