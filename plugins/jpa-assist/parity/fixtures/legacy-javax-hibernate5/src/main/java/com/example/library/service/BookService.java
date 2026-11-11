package com.example.library.service;

import com.example.library.domain.Book;
import com.example.library.repository.BookRepository;
import java.util.List;
import org.springframework.stereotype.Service;

@Service
public class BookService {

    private final BookRepository books;

    public BookService(BookRepository books) {
        this.books = books;
    }

    public List<Book> all() {
        return books.findAll();
    }
}
