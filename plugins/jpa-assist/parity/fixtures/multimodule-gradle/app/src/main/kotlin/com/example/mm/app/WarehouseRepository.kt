package com.example.mm.app

import com.example.mm.domain.Warehouse
import org.springframework.data.jpa.repository.JpaRepository
import org.springframework.data.jpa.repository.Query
import org.springframework.data.repository.query.Param

interface WarehouseRepository : JpaRepository<Warehouse, Long> {

    fun findByName(name: String): Warehouse?

    @Query("select w from Warehouse w where w.name = :name")
    fun lookup(@Param("name") name: String): Warehouse?
}
