package com.example.crm.service;

import com.example.crm.repository.ContactRepository;
import org.springframework.stereotype.Service;

@Service
public class ContactService {

    private final ContactRepository contacts;

    public ContactService(ContactRepository contacts) {
        this.contacts = contacts;
    }

    public long count() {
        return contacts.count();
    }
}
