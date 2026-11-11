// expect: jpa-column-length-type (length on int/BigDecimal), jpa-column-precision-type (precision/scale on String)
package com.example.shop.defects;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import java.math.BigDecimal;

@Entity
public class ColumnDefects {

    @Id
    private Long id;

    @Column(length = 10)
    private int quantity;

    @Column(length = 10)
    private BigDecimal amount;

    @Column(precision = 8, scale = 2)
    private String label;
}
