// expect: attribute-type x3 (@Temporal on LocalDate, @Enumerated on java.lang.String, @Version on java.lang.String)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Id;
import jakarta.persistence.Temporal;
import jakarta.persistence.TemporalType;
import jakarta.persistence.Version;
import java.time.LocalDate;

@Entity
public class AttributeTypeDefects {

    @Id
    private Long id;

    @Temporal(TemporalType.DATE)
    private LocalDate day;

    @Enumerated
    private java.lang.String kind;

    @Version
    private java.lang.String revision;
}
