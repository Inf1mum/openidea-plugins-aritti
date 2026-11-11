// expect: jpa-data-class
package com.example.mm.app.defects

import jakarta.persistence.Entity
import jakarta.persistence.Id

@Entity
data class KotlinDataDefect(
    @Id var id: Long? = null,
    var name: String = "",
) {
}
