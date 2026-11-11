// expect: jpa-missing-setter (info, annotated getter of a property-access entity)
package com.example.shop.defects;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class MissingSetterDefect {

    private Long id;

    private String name;

    @Id
    public Long getId() {
        return id;
    }

    public void setId(Long id) {
        this.id = id;
    }

    @Column(length = 40)
    public String getName() {
        return name;
    }
}
