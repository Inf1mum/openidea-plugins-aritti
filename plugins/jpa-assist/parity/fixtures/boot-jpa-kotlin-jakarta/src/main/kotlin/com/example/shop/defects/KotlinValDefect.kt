// expect: jpa-val-attribute (val name), jpa-immutable-collection (List on @OneToMany)
package com.example.shop.defects

import jakarta.persistence.Entity
import jakarta.persistence.FetchType
import jakarta.persistence.Id
import jakarta.persistence.OneToMany
import kotlin.Long
import kotlin.String
import kotlin.collections.List

@Entity
class KotlinValDefect {

    @Id
    var id: Long? = null

    val name: String = ""

    @OneToMany(mappedBy = "parent", fetch = FetchType.LAZY)
    var items: List<KotlinValDefect> = listOf()
}
