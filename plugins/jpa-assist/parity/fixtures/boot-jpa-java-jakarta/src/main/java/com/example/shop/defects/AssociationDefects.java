// expect: jpa-unidirectional-one-to-many, jpa-many-to-many-list, jpa-many-to-many-cascade-remove, jpa-eager-fetch, jpa-eager-to-one
package com.example.shop.defects;

import jakarta.persistence.CascadeType;
import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.Id;
import jakarta.persistence.ManyToMany;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.OneToMany;
import java.util.List;
import java.util.Set;

@Entity
public class AssociationDefects {

    @Id
    private Long id;

    @OneToMany
    private Set<NoArgCtorDefect> children;

    @ManyToMany
    private List<FinalEntityDefect> tags;

    @ManyToMany(cascade = CascadeType.REMOVE)
    private Set<FinalMemberDefect> peers;

    @ManyToOne(fetch = FetchType.EAGER)
    private NoArgCtorDefect eagerParent;

    @ManyToOne
    private FinalEntityDefect implicitEagerParent;
}
