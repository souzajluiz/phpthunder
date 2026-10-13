<?php

namespace App\Support;

use App\Models\User;

class Str
{
    public static function upper(string $value): string
    {
        return strtoupper($value);
    }
}

if (!function_exists('App\Support\current_user')) {
    function current_user(): User
    {
        return User::create();
    }
}
