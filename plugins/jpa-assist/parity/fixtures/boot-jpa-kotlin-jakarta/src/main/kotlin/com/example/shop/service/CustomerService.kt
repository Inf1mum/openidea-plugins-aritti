package com.example.shop.service

import com.example.shop.domain.Customer
import com.example.shop.repository.CustomerRepository
import org.springframework.stereotype.Service
import org.springframework.transaction.annotation.Transactional

@Service
class CustomerService(private val customers: CustomerRepository) {

    @Transactional(readOnly = true)
    fun findAll(): List<Customer> = customers.findAll()
}
