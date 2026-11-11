// expect: jpa-multiple-id-kinds
package com.example.shop.defects;

import com.example.shop.domain.OrderLineId;
import jakarta.persistence.EmbeddedId;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class MultipleIdKindsDefect {

    @Id
    private Long id;

    @EmbeddedId
    private OrderLineId compositeId;
}
