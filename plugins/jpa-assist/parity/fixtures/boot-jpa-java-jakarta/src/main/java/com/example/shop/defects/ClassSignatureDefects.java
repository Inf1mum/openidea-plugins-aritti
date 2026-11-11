// expect: jpa-entity-multiple-roles (@Entity + @Embeddable), jpa-entity-nested (non-static nested entity)
package com.example.shop.defects;

import jakarta.persistence.Embeddable;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
@Embeddable
public class ClassSignatureDefects {

    @Id
    private Long id;

    @Entity
    public class NestedEntity {

        @Id
        private Long id;
    }
}
