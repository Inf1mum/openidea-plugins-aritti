// expect: jpa-inheritance-strategy-not-root (strategy repeated on a subclass of Payment)
package com.example.shop.defects;

import com.example.shop.domain.Payment;
import jakarta.persistence.Entity;
import jakarta.persistence.Inheritance;
import jakarta.persistence.InheritanceType;

@Entity
@Inheritance(strategy = InheritanceType.SINGLE_TABLE)
public class InheritanceChild extends Payment {
}
