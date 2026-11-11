// expect: jpa-source-identifier-absent
package com.example.library.defects;

import javax.persistence.Column;
import javax.persistence.Entity;

@Entity
public class JavaxMissingIdDefect {

    @Column
    private String name;
}
