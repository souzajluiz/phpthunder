<?php

namespace App\Models;

use App\Contracts\HasName;
use App\Support\Greets;

final class User extends Model implements HasName
{
    use Greets;

    public const ROLE = 'user';

    public function __construct(private Address $address, public string $name = '')
    {
    }

    public function getName(): string
    {
        return $this->name;
    }

    public function address(): Address
    {
        return $this->address;
    }
}
