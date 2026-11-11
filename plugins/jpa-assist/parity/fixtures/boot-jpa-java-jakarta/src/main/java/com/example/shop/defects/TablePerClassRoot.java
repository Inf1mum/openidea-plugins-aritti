// expect: jpa-discriminator-table-per-class
package com.example.shop.defects;

import jakarta.persistence.DiscriminatorColumn;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Inheritance;
import jakarta.persistence.InheritanceType;

@Entity
@Inheritance(strategy = InheritanceType.TABLE_PER_CLASS)
@DiscriminatorColumn(name = "kind")
public class TablePerClassRoot {

    @Id
    private Long id;
}
