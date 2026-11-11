package com.example.mm.app

import com.example.mm.domain.Warehouse
import org.springframework.stereotype.Service

@Service
class WarehouseService(private val warehouses: WarehouseRepository) {

    fun all(): List<Warehouse> = warehouses.findAll()
}
