package com.example.library.domain;

import java.util.HashSet;
import java.util.Set;
import javax.persistence.Entity;
import javax.persistence.Id;
import javax.persistence.ManyToMany;

@Entity
public class Tag {

    @Id
    private String label;

    @ManyToMany(mappedBy = "tags")
    private Set<Book> books = new HashSet<>();

    protected Tag() {
    }

    public String getLabel() {
        return label;
    }

    public Set<Book> getBooks() {
        return books;
    }
}
