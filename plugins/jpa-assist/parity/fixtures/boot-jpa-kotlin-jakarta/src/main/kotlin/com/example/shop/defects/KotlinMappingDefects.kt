// expect: jpa-generated-value-without-id, jpa-generated-value-type, jpa-column-length-type, jpa-column-precision-type, jpa-unidirectional-one-to-many, jpa-many-to-many-list
package com.example.shop.defects

import jakarta.persistence.CascadeType
import jakarta.persistence.Column
import jakarta.persistence.Entity
import jakarta.persistence.FetchType
import jakarta.persistence.GeneratedValue
import jakarta.persistence.GenerationType
import jakarta.persistence.Id
import jakarta.persistence.ManyToMany
import jakarta.persistence.ManyToOne
import jakarta.persistence.OneToMany

@Entity
class KotlinMappingDefects {

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    var id: Long? = null

    @GeneratedValue(strategy = GenerationType.SEQUENCE)
    var ticket: Long? = null

    @Column(length = 10)
    var quantity: Int = 0

    @Column(precision = 8, scale = 2)
    var label: String = ""

    @OneToMany
    var children: MutableSet<KotlinMappingDefects> = mutableSetOf()

    @ManyToMany
    var tags: MutableList<KotlinMappingDefects> = mutableListOf()

    @ManyToOne(fetch = FetchType.EAGER)
    var eagerParent: KotlinMappingDefects? = null

    @ManyToOne
    var implicitEagerParent: KotlinMappingDefects? = null
}
