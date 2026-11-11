// expect: jpa-duplicate-embedded-id
package com.example.shop.defects;

import com.example.shop.domain.OrderLineId;
import jakarta.persistence.EmbeddedId;
import jakarta.persistence.Entity;

@Entity
public class DuplicateEmbeddedIdDefect {

    @EmbeddedId
    private OrderLineId first;

    @EmbeddedId
    private OrderLineId second;
}
