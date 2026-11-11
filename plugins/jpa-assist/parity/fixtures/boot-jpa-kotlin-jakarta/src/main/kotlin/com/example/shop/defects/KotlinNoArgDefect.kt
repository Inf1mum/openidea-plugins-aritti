// expect: none live. The Kotlin build rules (jpa-redundant-open, jpa-kotlin-plugin-missing, jpa-redundant-constructor) are not in the live provider path; see GOLDEN.md.
// Intended manual checks: 'open' is redundant (allOpen names jakarta.persistence.Entity) and the empty secondary constructor is redundant (plugin.jpa applied).
package com.example.shop.defects

import jakarta.persistence.Entity
import jakarta.persistence.Id

@Entity
open class KotlinNoArgDefect(
    @Id var id: Long,
    var name: String,
) {
    constructor() : this(0, "")
}
