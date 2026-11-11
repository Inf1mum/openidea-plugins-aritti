// expect: attribute-type (Temporal on LocalDate)
package com.example.library.defects;

import java.time.LocalDate;
import javax.persistence.Entity;
import javax.persistence.Id;
import javax.persistence.Temporal;
import javax.persistence.TemporalType;

@Entity
public class JavaxAttributeTypeDefects {

    @Id
    private Long id;

    @Temporal(TemporalType.DATE)
    private LocalDate day;
}
