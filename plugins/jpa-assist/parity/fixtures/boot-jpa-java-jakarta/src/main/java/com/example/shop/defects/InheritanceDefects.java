// expect: jpa-entity-mapped-superclass, jpa-entity-multiple-roles (@Entity + @MappedSuperclass). See also DiscriminatorNotEntity, TablePerClassRoot, InheritanceChild.
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.MappedSuperclass;

@Entity
@MappedSuperclass
public class InheritanceDefects {

    @Id
    private Long id;
}
