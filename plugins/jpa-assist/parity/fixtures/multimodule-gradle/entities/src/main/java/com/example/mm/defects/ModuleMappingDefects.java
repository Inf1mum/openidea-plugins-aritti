// expect: jpa-final-member, jpa-column-length-type, jpa-eager-to-one, jpa-many-to-many-list
package com.example.mm.defects;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.ManyToMany;
import jakarta.persistence.ManyToOne;
import java.util.List;

@Entity
public class ModuleMappingDefects {

    @Id
    private Long id;

    private final String label = "x";

    @Column(length = 10)
    private int quantity;

    @ManyToOne
    private ModuleNoArgDefect parent;

    @ManyToMany
    private List<ModuleNoArgDefect> peers;
}
