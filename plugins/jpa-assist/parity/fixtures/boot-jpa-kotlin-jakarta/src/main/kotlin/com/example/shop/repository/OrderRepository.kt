package com.example.shop.repository

import com.example.shop.domain.CustomerOrder
import com.example.shop.domain.OrderStatus
import org.springframework.data.jpa.repository.JpaRepository

interface OrderRepository : JpaRepository<CustomerOrder, Long> {

    fun findByStatus(status: OrderStatus): List<CustomerOrder>
}
