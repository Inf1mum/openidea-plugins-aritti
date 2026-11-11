// expect: jpa-mixed-access (info, mapping annotation on getter while @Id is on a field)
package com.example.shop.defects;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class MixedAccessDefect {

    @Id
    private Long id;

    private String name;

    @Column(length = 40)
    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }
}
