// expect: jpa-generated-value-type (IDENTITY on a String identifier)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;

@Entity
public class GeneratedValueStringDefect {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private String code;
}
