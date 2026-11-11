package com.example.shop.service;

import com.example.shop.domain.Customer;
import com.example.shop.repository.CustomerRepository;
import java.util.List;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class CustomerService {

    private final CustomerRepository customers;

    public CustomerService(CustomerRepository customers) {
        this.customers = customers;
    }

    @Transactional(readOnly = true)
    public List<Customer> findAll() {
        return customers.findAll();
    }
}
