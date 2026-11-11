// expect: jpa-generated-value-without-id (on 'ticket'), jpa-generated-value-type (on 'uuidAsLong' strategy and 'code' strategy)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;

@Entity
public class GeneratedValueDefects {

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    private Long uuidAsLong;

    @GeneratedValue(strategy = GenerationType.SEQUENCE)
    private Long ticket;
}
