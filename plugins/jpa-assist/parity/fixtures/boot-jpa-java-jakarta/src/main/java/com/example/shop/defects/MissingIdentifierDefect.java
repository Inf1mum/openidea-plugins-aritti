// expect: jpa-source-identifier-absent (info)
package com.example.shop.defects;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;

@Entity
public class MissingIdentifierDefect {

    @Column
    private String name;
}
