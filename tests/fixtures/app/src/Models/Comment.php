<?php

namespace App\Models;

class Comment
{
    public function body(): string
    {
        return '';
    }

    /**
     * @return User
     */
    public function writer()
    {
        return User::create();
    }
}
