// expect: jpa-no-arg-constructor
package com.example.library.defects;

import javax.persistence.Entity;
import javax.persistence.Id;

@Entity
public class JavaxNoArgCtorDefect {

    @Id
    private Long id;

    public JavaxNoArgCtorDefect(Long id) {
        this.id = id;
    }
}
