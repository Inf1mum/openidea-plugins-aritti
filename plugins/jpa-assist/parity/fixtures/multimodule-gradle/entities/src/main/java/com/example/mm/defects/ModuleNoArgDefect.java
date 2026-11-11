// expect: jpa-no-arg-constructor (Java module "entities"; open the entities/ folder as its own project to see it, see README)
package com.example.mm.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class ModuleNoArgDefect {

    @Id
    private Long id;

    public ModuleNoArgDefect(Long id) {
        this.id = id;
    }
}
