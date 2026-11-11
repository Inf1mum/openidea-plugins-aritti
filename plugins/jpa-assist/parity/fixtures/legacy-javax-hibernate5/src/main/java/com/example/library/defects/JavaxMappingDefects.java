// expect: jpa-final-member, jpa-eager-fetch, jpa-eager-to-one, jpa-column-length-type, jpa-unidirectional-one-to-many, jpa-many-to-many-list, jpa-generated-value-type
package com.example.library.defects;

import java.util.List;
import java.util.Set;
import javax.persistence.Column;
import javax.persistence.Entity;
import javax.persistence.FetchType;
import javax.persistence.GeneratedValue;
import javax.persistence.GenerationType;
import javax.persistence.Id;
import javax.persistence.ManyToMany;
import javax.persistence.ManyToOne;
import javax.persistence.OneToMany;

@Entity
public class JavaxMappingDefects {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private String id;

    private final String label = "x";

    @Column(length = 10)
    private int quantity;

    @OneToMany
    private Set<JavaxNoArgCtorDefect> children;

    @ManyToMany
    private List<JavaxNoArgCtorDefect> peers;

    @ManyToOne(fetch = FetchType.EAGER)
    private JavaxNoArgCtorDefect eagerParent;

    @ManyToOne
    private JavaxNoArgCtorDefect implicitEagerParent;
}
