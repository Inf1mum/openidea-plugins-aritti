// expect: jpa-discriminator-not-entity
package com.example.shop.defects;

import jakarta.persistence.DiscriminatorColumn;

@DiscriminatorColumn(name = "kind")
public class DiscriminatorNotEntity {
}
