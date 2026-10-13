<?php

namespace App\Models;

abstract class Model
{
    protected static string $table = '';

    public static function create(array $attributes = []): static
    {
        return new static();
    }

    public function save(): bool
    {
        return true;
    }
}
