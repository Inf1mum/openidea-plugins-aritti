// expect: jpa-final-member (twice: final field, final method)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class FinalMemberDefect {

    @Id
    private Long id;

    private final String label = "x";

    public final String describe() {
        return label;
    }
}
