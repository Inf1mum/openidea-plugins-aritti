// expect: jpa-lombok-data, jpa-lombok-to-string (customer, lines), jpa-lombok-equals-field (customer, lines, generated id)
package com.example.crm.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.Id;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.OneToMany;
import java.util.List;
import lombok.Data;

@Entity
@Data
public class LombokDataDefect {

    @Id
    @GeneratedValue
    private Long id;

    @ManyToOne(fetch = FetchType.LAZY)
    private LombokDataDefect parent;

    @OneToMany(mappedBy = "parent")
    private List<LombokDataDefect> children;

    private String note;
}
