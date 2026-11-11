// expect: jpa-embeddable-identifier
package com.example.shop.defects;

import jakarta.persistence.Embeddable;
import jakarta.persistence.Id;

@Embeddable
public class EmbeddableIdentifierDefect {

    @Id
    private Long id;

    private String value;
}
