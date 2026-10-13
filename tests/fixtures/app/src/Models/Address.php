<?php

namespace App\Models;

class Address
{
    public string $city = '';

    public function format(): string
    {
        return $this->city;
    }
}
