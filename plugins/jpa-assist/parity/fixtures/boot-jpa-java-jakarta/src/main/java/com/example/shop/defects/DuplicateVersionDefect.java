// expect: jpa-duplicate-version
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Version;

@Entity
public class DuplicateVersionDefect {

    @Id
    private Long id;

    @Version
    private long first;

    @Version
    private long second;
}
