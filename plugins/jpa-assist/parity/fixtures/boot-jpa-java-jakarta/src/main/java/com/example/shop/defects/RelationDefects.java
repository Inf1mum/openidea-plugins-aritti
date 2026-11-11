// expect: jpa-attribute-multiple-relations (on @ManyToOne), jpa-relationship-type (@OneToMany on String, @ManyToOne on List)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.Id;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.OneToMany;
import java.util.List;

@Entity
public class RelationDefects {

    @Id
    private Long id;

    @OneToMany(mappedBy = "x")
    @ManyToOne(fetch = FetchType.LAZY)
    private List<NoArgCtorDefect> both;

    @OneToMany(mappedBy = "x")
    private java.lang.String notACollection;

    @ManyToOne(fetch = FetchType.LAZY)
    private List<NoArgCtorDefect> collectionForToOne;
}
