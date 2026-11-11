package com.example.shop.repository

import com.example.shop.domain.Customer
import com.example.shop.dto.CustomerNameView
import com.example.shop.dto.CustomerSummary
import org.springframework.data.domain.Page
import org.springframework.data.domain.Pageable
import org.springframework.data.domain.Slice
import org.springframework.data.jpa.repository.EntityGraph
import org.springframework.data.jpa.repository.JpaRepository
import org.springframework.data.jpa.repository.Query
import org.springframework.data.repository.query.Param

interface CustomerRepository : JpaRepository<Customer, Long> {

    fun findByEmail(email: String): Customer?

    fun findByNameContainingIgnoreCaseOrderByNameAsc(fragment: String): List<Customer>

    fun findByAddressCity(city: String, pageable: Pageable): Page<Customer>

    fun findByNameStartingWith(prefix: String, pageable: Pageable): Slice<Customer>

    fun existsByEmail(email: String): Boolean

    @EntityGraph("Customer.withOrders")
    fun findWithOrdersById(id: Long): Customer?

    @Query("select c from Customer c where c.email = :email and c.name = :name")
    fun lookup(@Param("email") email: String, @Param("name") name: String): Customer?

    @Query("select c.name as name, c.email as email from Customer c where c.id = :id")
    fun projectNameById(@Param("id") id: Long): CustomerNameView?

    @Query("select new com.example.shop.dto.CustomerSummary(c.id, c.name) from Customer c")
    fun summaries(): List<CustomerSummary>
}
