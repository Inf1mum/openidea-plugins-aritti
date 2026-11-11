// expect: jpa-data-class
package com.example.shop.defects

import jakarta.persistence.Entity
import jakarta.persistence.Id

@Entity
data class KotlinDataClassDefect(
    @Id var id: Long? = null,
    var name: String = "",
) {
}
